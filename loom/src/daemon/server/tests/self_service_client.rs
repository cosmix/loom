//! End-to-end authorization for the three RPCs a stage agent may make about
//! its OWN stage, driven through `handle_client_connection`.
//!
//! `SO_PEERCRED` on a `UnixStream::pair` reports THIS process, so a session
//! whose recorded pid is `std::process::id()` is one the test genuinely runs
//! inside — the only way to exercise the accepting side of peer identity
//! without spawning a process tree. A session recorded at pid 1 is the honest
//! negative: live, valid, and not us.

use super::*;
use crate::daemon::protocol::read_message;
use crate::daemon::server::tokens::USER_TOKEN_FILE;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::{load_stage, save_stage};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::thread;
use tempfile::TempDir;

/// The credential a sandboxed agent presents: non-empty, because the wire
/// preface refuses to frame an empty one, and not the token, which it is
/// denied reading.
const NO_TOKEN: &str = "peer-identity";

/// A work directory NAMED `.work`.
///
/// The dispute handler resolves its own `WorkDir`, whose upward search would
/// otherwise climb out of a differently-named fixture and find the real
/// project's `.work`.
fn work_root(temp: &TempDir) -> PathBuf {
    let work_dir = temp.path().join(".work");
    std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    // A token exists — the point is that the caller cannot read it.
    std::fs::write(work_dir.join(USER_TOKEN_FILE), "user-secret").unwrap();
    work_dir
}

/// An executing stage and the running session that owns it, with `session_pid`
/// recorded as the session's own process.
///
/// Passing `std::process::id()` makes THIS test process genuinely inside the
/// session, which is what `SO_PEERCRED` reports for a `UnixStream::pair` and
/// therefore the only way to exercise the accepting side of peer identity.
fn stage_owned_by(work_dir: &Path, stage_id: &str, session_pid: u32) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = SessionStatus::Running;
    session.pid = Some(session_pid);
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    stage.acceptance = vec![AcceptanceCriterion::Simple("echo verified".to_string())];
    save_stage(&stage, work_dir).unwrap();
    save_session(&session, work_dir).unwrap();
    write_test_pid_identity(work_dir, &session, session_pid).unwrap();
    session
}

/// Serve exactly one request over a socket pair and return the reply.
fn serve_one(work_dir: &Path, request: &Request) -> Response {
    let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
    let handler = {
        let work_dir = work_dir.to_path_buf();
        thread::spawn(move || {
            handle_client_connection(
                server_stream,
                Arc::new(AtomicBool::new(false)),
                Arc::new(Mutex::new(Vec::new())),
                Arc::new(Mutex::new(Vec::new())),
                &work_dir,
                ByteBudget::new(2 * crate::daemon::wire::MAX_REQUEST_BYTES),
            )
        })
    };
    write_message(&mut client_stream, request).unwrap();
    let response = read_message(&mut client_stream).unwrap();
    drop(client_stream);
    handler.join().unwrap().unwrap();
    response
}

fn block_request(stage_id: &str, session_id: &str) -> Request {
    Request::BlockStage {
        auth_token: NO_TOKEN.to_string(),
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        reason: "criterion 41 is unrunnable".to_string(),
    }
}

fn dispute_request(stage_id: &str, session_id: &str) -> Request {
    Request::DisputeCriteria {
        auth_token: NO_TOKEN.to_string(),
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        criterion_index: 0,
        reason: "criterion 41 contradicts its own data source".to_string(),
        evidence_commit: None,
        failure_output: None,
    }
}

/// The incident this whole path exists for: an agent that cannot read
/// `.work/user.token` and cannot write `.work/stages/` records its own
/// blockage anyway.
#[test]
fn a_token_less_block_from_inside_the_session_is_applied() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    let session = stage_owned_by(&work_dir, "build-api", std::process::id());

    let response = serve_one(&work_dir, &block_request("build-api", &session.id));

    assert!(matches!(response, Response::Ok), "got {response:?}");
    let stage = load_stage("build-api", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(
        stage.close_reason.as_deref(),
        Some("criterion 41 is unrunnable")
    );
}

#[test]
fn a_token_less_block_from_outside_the_named_session_is_refused() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    // pid 1 is nobody's descendant in the walking-up direction, so this
    // session is live and valid and simply is not us.
    let theirs = stage_owned_by(&work_dir, "other-stage", 1);

    let response = serve_one(&work_dir, &block_request("other-stage", &theirs.id));

    assert!(
        matches!(response, Response::AuthenticationFailed),
        "got {response:?}"
    );
    assert_eq!(
        load_stage("other-stage", &work_dir).unwrap().status,
        StageStatus::Executing,
        "a refused block must not touch the stage"
    );
}

/// The escalation the ownership check exists to stop: a caller that IS inside
/// a real session, naming a stage that session does not own.
#[test]
fn a_block_naming_another_sessions_stage_is_refused() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    let mine = stage_owned_by(&work_dir, "build-api", std::process::id());
    stage_owned_by(&work_dir, "other-stage", std::process::id());

    let response = serve_one(&work_dir, &block_request("other-stage", &mine.id));

    assert!(
        matches!(response, Response::AuthenticationFailed),
        "got {response:?}"
    );
    assert_eq!(
        load_stage("other-stage", &work_dir).unwrap().status,
        StageStatus::Executing
    );
}

/// A valid token is not an exemption. It says the caller is an operator; it
/// says nothing about whether the session it named owns the stage it named.
#[test]
fn a_valid_token_still_cannot_act_on_a_stage_the_named_session_lacks() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    let mine = stage_owned_by(&work_dir, "build-api", std::process::id());
    stage_owned_by(&work_dir, "other-stage", std::process::id());

    let mut request = block_request("other-stage", &mine.id);
    if let Request::BlockStage { auth_token, .. } = &mut request {
        *auth_token = "user-secret".to_string();
    }
    let response = serve_one(&work_dir, &request);

    assert!(
        matches!(response, Response::AuthenticationFailed),
        "got {response:?}"
    );
}

/// An operator shell holds the token and is inside no session, so it names
/// none — and there is nothing for the ownership check to prove.
#[test]
fn an_operator_with_the_token_and_no_session_may_block() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    stage_owned_by(&work_dir, "build-api", std::process::id());

    let mut request = block_request("build-api", "");
    if let Request::BlockStage { auth_token, .. } = &mut request {
        *auth_token = "user-secret".to_string();
    }
    let response = serve_one(&work_dir, &request);

    assert!(matches!(response, Response::Ok), "got {response:?}");
    assert_eq!(
        load_stage("build-api", &work_dir).unwrap().status,
        StageStatus::Blocked
    );
}

/// The other half of the incident: the same agent disputing a defective
/// acceptance criterion.
#[test]
fn a_token_less_dispute_from_inside_the_session_is_filed() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    let session = stage_owned_by(&work_dir, "build-api", std::process::id());

    let response = serve_one(&work_dir, &dispute_request("build-api", &session.id));

    assert!(
        matches!(response, Response::DisputeCreated { id: 1 }),
        "got {response:?}"
    );
    assert_eq!(
        load_stage("build-api", &work_dir).unwrap().status,
        StageStatus::NeedsAdjudication
    );
}

#[test]
fn a_token_less_dispute_from_outside_the_named_session_is_refused() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    let theirs = stage_owned_by(&work_dir, "other-stage", 1);

    let response = serve_one(&work_dir, &dispute_request("other-stage", &theirs.id));

    assert!(
        matches!(response, Response::AuthenticationFailed),
        "got {response:?}"
    );
    assert_eq!(
        load_stage("other-stage", &work_dir).unwrap().status,
        StageStatus::Executing
    );
    assert!(!work_dir.join("disputes/other-stage").exists());
}

#[test]
fn a_dispute_naming_another_sessions_stage_is_refused() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    let mine = stage_owned_by(&work_dir, "build-api", std::process::id());
    stage_owned_by(&work_dir, "other-stage", std::process::id());

    let response = serve_one(&work_dir, &dispute_request("other-stage", &mine.id));

    assert!(
        matches!(response, Response::AuthenticationFailed),
        "got {response:?}"
    );
    assert_eq!(
        load_stage("other-stage", &work_dir).unwrap().status,
        StageStatus::Executing
    );
    assert!(!work_dir.join("disputes/other-stage").exists());
}

/// The default the exception list is there to preserve: a User RPC outside the
/// self-service set gets nothing from being inside a session.
#[test]
fn a_user_request_outside_the_self_service_set_still_needs_the_token() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_root(&temp);
    stage_owned_by(&work_dir, "build-api", std::process::id());

    let response = serve_one(
        &work_dir,
        &Request::Ping {
            auth_token: NO_TOKEN.to_string(),
        },
    );

    assert!(
        matches!(response, Response::AuthenticationFailed),
        "got {response:?}"
    );
}
