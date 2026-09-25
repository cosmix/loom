//! The daemon as the one observer: the fingerprint it serves, the client
//! that asks for it over the socket, who may ask, and the completion gates it
//! runs in its own view.

use super::*;
use crate::daemon::protocol::{read_message, write_message, Request};
use crate::daemon::server::admission::ByteBudget;
use crate::daemon::server::client::handle_client_connection;
use crate::daemon::server::lock::acquire_lock;
use crate::daemon::server::tokens::USER_TOKEN_FILE;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::verify::contracts::test_support::{
    contract_worktree, pinned, plant_foreign_git_dir, CONTRACT_FILE,
};
use crate::verify::review::store::{write_round, ReviewRound, RECORD_VERSION};
use crate::verify::tool_artifacts::NAMES;
use crate::verify::transitions::{save_stage, update_stage};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const STAGE: &str = "s1";

struct Fixture {
    _temp: TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
}

/// A project on `main` whose executing v2 standard stage `s1` has its
/// worktree at `.worktrees/s1` with one untracked contract file, and a state
/// directory holding the user token. No daemon listens, and the singleton
/// lock a stopped daemon left proves none runs.
fn fixture() -> Fixture {
    let temp = TempDir::new().unwrap();
    let project = temp.path().join("repo");
    let worktree = contract_worktree(&project, STAGE);
    let work_dir = project.join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
    std::fs::write(work_dir.join(USER_TOKEN_FILE), "user-secret").unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        stage_type: StageType::Standard,
        worktree: Some(STAGE.to_string()),
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    Fixture {
        _temp: temp,
        work_dir,
        worktree,
    }
}

fn local(fx: &Fixture) -> ChangeFingerprint {
    fingerprint::compute_local(&pinned(&fx.worktree), "main").unwrap()
}

/// Serve one connection the way the daemon's accept loop does.
fn serve(stream: UnixStream, work_dir: &Path) {
    handle_client_connection(
        stream,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
        work_dir,
        ByteBudget::new(2 * crate::daemon::MAX_REQUEST_BYTES),
    )
    .unwrap();
}

#[test]
fn the_handler_serves_the_daemons_own_fingerprint() {
    let fx = fixture();

    match handle_observe_changes(&fx.work_dir, STAGE) {
        Response::ChangesObserved {
            target_branch,
            fingerprint,
        } => {
            assert_eq!(target_branch, "main");
            assert_eq!(fingerprint, local(&fx));
            assert!(fingerprint.files.contains_key(CONTRACT_FILE));
        }
        other => panic!("expected ChangesObserved, got {other:?}"),
    }
}

/// The request names a stage, never a path: a traversal shape dies before
/// any file is touched.
#[test]
fn the_handler_refuses_a_stage_id_shaped_like_a_path() {
    let fx = fixture();

    match handle_observe_changes(&fx.work_dir, "../../etc") {
        Response::Error { message } => assert!(message.contains("invalid stage id"), "{message}"),
        other => panic!("expected an error, got {other:?}"),
    }
}

/// `fingerprint::compute` outside the daemon asks it over the socket and gets
/// what the daemon computes; a target branch other than the stage's is
/// refused rather than measured locally.
#[test]
fn a_client_gets_the_daemons_fingerprint_over_the_socket() {
    let fx = fixture();
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::unix_socket_bindable(&fx.work_dir),
        "daemon::server::observer::tests::a_client_gets_the_daemons_fingerprint_over_the_socket",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    let listener = UnixListener::bind(fx.work_dir.join("orchestrator.sock")).unwrap();
    let work_dir = fx.work_dir.clone();
    let daemon = std::thread::spawn(move || {
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            serve(stream, &work_dir);
        }
    });

    let observed = fingerprint::compute(&fx.worktree, "main").unwrap();
    let other_target = fingerprint::compute(&fx.worktree, "feature").unwrap_err();
    daemon.join().unwrap();

    assert_eq!(observed, local(&fx));
    let message = format!("{other_target:#}");
    assert!(
        message.contains("measures this stage's changes against 'main'"),
        "{message}"
    );
}

/// A running session named `stage_id`'s owner, recorded at this process's
/// pid so the connection's peer identity proves the caller is inside it.
fn session_owning(work_dir: &Path, stage_id: &str) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = SessionStatus::Running;
    session.pid = Some(std::process::id());
    save_session(&session, work_dir).unwrap();
    write_test_pid_identity(work_dir, &session, std::process::id()).unwrap();
    session
}

/// Ask over a socket pair with no token, as a sandboxed agent would.
fn ask_without_token(work_dir: &Path, session_id: &str) -> Response {
    let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
    let handler = {
        let work_dir = work_dir.to_path_buf();
        std::thread::spawn(move || serve(server_stream, &work_dir))
    };
    let request = Request::ObserveChanges {
        auth_token: "peer-identity".to_string(),
        stage_id: STAGE.to_string(),
        session_id: session_id.to_string(),
    };
    write_message(&mut client_stream, &request).unwrap();
    let response = read_message(&mut client_stream).unwrap();
    drop(client_stream);
    handler.join().unwrap();
    response
}

/// Without the token, only the stage's own running session may ask.
#[test]
fn a_token_less_caller_may_observe_only_its_own_stage() {
    let fx = fixture();
    let stranger = session_owning(&fx.work_dir, "other");
    save_stage(
        &Stage {
            id: "other".to_string(),
            session: Some(stranger.id.clone()),
            ..Stage::default()
        },
        &fx.work_dir,
    )
    .unwrap();

    let refused = ask_without_token(&fx.work_dir, &stranger.id);
    assert!(
        matches!(refused, Response::AuthenticationFailed),
        "{refused:?}"
    );

    let owner = session_owning(&fx.work_dir, STAGE);
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.session = Some(owner.id.clone());
        Ok(())
    })
    .unwrap();
    let answered = ask_without_token(&fx.work_dir, &owner.id);
    assert!(
        matches!(answered, Response::ChangesObserved { .. }),
        "{answered:?}"
    );
}

/// A well-formed round at `seen`, with no findings.
fn round_at(seen: &ChangeFingerprint) -> ReviewRound {
    ReviewRound {
        version: RECORD_VERSION,
        round: 1,
        agent_id: "agent-1".to_string(),
        harvested_at: chrono::Utc::now(),
        fingerprint: seen.value.clone(),
        files: seen.files.clone(),
        malformed: None,
        findings: Vec::new(),
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    }
}

/// The observed failure, end to end: the review round was recorded while the
/// sandbox's placeholders stood empty at the worktree root, and the daemon ran
/// the completion gates once they were gone. A real change after the round
/// still fails the gate.
#[test]
fn a_round_recorded_among_placeholders_passes_the_completion_gates() {
    let fx = fixture();
    for name in NAMES {
        std::fs::write(fx.worktree.join(name), b"").unwrap();
    }
    let seen = fingerprint::compute(&fx.worktree, "main").unwrap();
    write_round(&fx.work_dir, STAGE, &round_at(&seen)).unwrap();
    for name in NAMES {
        std::fs::remove_file(fx.worktree.join(name)).unwrap();
    }

    check_completion_gates(&fx.work_dir, STAGE).unwrap();

    std::fs::write(fx.worktree.join("late.rs"), b"fn late() {}\n").unwrap();
    let error = check_completion_gates(&fx.work_dir, STAGE).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("changed since: late.rs"), "{message}");
}

/// The message `fingerprint::compute` fails with when it may not compute.
fn refusal_to_compute(fx: &Fixture) -> String {
    let error = fingerprint::compute(&fx.worktree, "main").unwrap_err();
    format!("{error:#}")
}

/// No socket and a free singleton lock: no daemon runs, so a client computes
/// the fingerprint itself, as the daemon would.
#[test]
fn a_free_lock_and_no_socket_let_a_client_compute_locally() {
    let fx = fixture();

    assert_eq!(
        fingerprint::compute(&fx.worktree, "main").unwrap(),
        local(&fx)
    );
}

/// No socket, but a daemon holds the lock: it runs and this process cannot
/// reach it, so nothing is computed here.
#[test]
fn a_held_lock_without_a_socket_fails_closed() {
    let fx = fixture();
    let _daemon = acquire_lock(&fx.work_dir).unwrap();

    let message = refusal_to_compute(&fx);

    assert!(message.contains("cannot prove that none runs"), "{message}");
}

/// No socket and no lock file: a sandbox that hides the state directory looks
/// exactly like this, so absence proves nothing.
#[test]
fn a_missing_lock_is_no_proof_that_no_daemon_runs() {
    let fx = fixture();
    std::fs::remove_file(fx.work_dir.join("orchestrator.lock")).unwrap();

    let message = refusal_to_compute(&fx);

    assert!(message.contains("cannot prove that none runs"), "{message}");
}

/// A lock that is no regular file (here a directory, which opens and takes
/// a `flock`) proves nothing either, as `/dev/null` mounted over it would not.
#[test]
fn a_lock_that_is_no_regular_file_is_no_proof() {
    let fx = fixture();
    let lock = fx.work_dir.join("orchestrator.lock");
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();

    let message = refusal_to_compute(&fx);

    assert!(message.contains("cannot prove that none runs"), "{message}");
}

/// The stage repointed its `.git` file at a git directory whose
/// configuration defines a clean filter: the daemon fingerprints from the
/// stage's registered git directory and never runs the filter.
#[test]
fn the_daemon_fingerprint_ignores_a_repointed_git_file() {
    let fx = fixture();
    let marker = plant_foreign_git_dir(&fx.worktree);

    let fingerprint = match handle_observe_changes(&fx.work_dir, STAGE) {
        Response::ChangesObserved { fingerprint, .. } => fingerprint,
        other => panic!("expected ChangesObserved, got {other:?}"),
    };
    // The integrity scan runs its git first; only the review gate fails.
    let gates = check_completion_gates(&fx.work_dir, STAGE).unwrap_err();

    assert!(!marker.exists(), "the daemon ran the stage's clean filter");
    let message = format!("{gates:#}");
    assert!(message.contains("no well-formed review round"), "{message}");
    let files: Vec<&String> = fingerprint.files.keys().collect();
    assert_eq!(files, [".gitattributes", "README.md", CONTRACT_FILE]);
}
