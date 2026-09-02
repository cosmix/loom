//! Unit tests for the session write-ahead invariant in [`super`]: a stage is
//! `Executing` only if `stage.session` names a session record that exists on
//! disk, and the executor never spawns a second agent over one that is still
//! alive.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage, update_stage};
use std::path::Path;
use tempfile::TempDir;

/// A `.loom/work` directory whose configured terminal lane is tmux, so
/// `Orchestrator::new` never runs real terminal detection (which fails on a
/// headless test runner). Same trick `tests_session_registry.rs` uses for the
/// same reason.
fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    write_terminal_config(
        &work,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    temp
}

fn orchestrator_for(work_dir: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap()
}

fn stage_at(work_dir: &Path, stage_id: &str, status: StageStatus) {
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = status;
    save_stage(&stage, work_dir).unwrap();
}

fn session_for(stage_id: &str, status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = status;
    session
}

/// The PID file the wrapper script writes at spawn, naming this test process
/// so the identity probe answers `VerifiedAlive`.
fn spawn_a_live_agent(work_dir: &Path, session: &Session) {
    write_test_pid_identity(work_dir, session, std::process::id()).unwrap();
}

/// The bug this pins (see stage_executor.rs): a daemon crash leaves a stage
/// `Executing` with a live-but-unreachable session, and `loom stage reset` (or
/// any path that walks the stage back to `Queued`) must not cause
/// `start_stage` to spawn a second agent into the same worktree. It must
/// adopt the still-live session instead.
#[test]
fn start_stage_adopts_a_live_session_instead_of_spawning_a_duplicate() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");

    // Simulates the post-`loom stage reset` state: back to Queued, but the
    // first agent never actually stopped.
    stage_at(&work, "alpha", StageStatus::Queued);
    let live = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(&work, &live);
    save_session(&live, &work).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.start_stage("alpha").unwrap();

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(
        after.status,
        StageStatus::Executing,
        "adopting a live session must still move the stage into Executing"
    );
    assert_eq!(
        after.session.as_deref(),
        Some(live.id.as_str()),
        "the stage must link to the live session, not spawn a new one"
    );

    let tracked = orchestrator
        .active_sessions
        .get("alpha")
        .expect("the adopted session must be tracked so the monitor watches it");
    assert_eq!(tracked.id, live.id);

    let session_files: Vec<_> = std::fs::read_dir(work.join("sessions"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        session_files.len(),
        1,
        "adopting must not write a second session record"
    );
}

/// Adoption must use the stage's OWN worker kind, not "newest live session
/// found": an adjudication session carries the stage's own `stage_id` and
/// would otherwise win by being spawned later.
#[test]
fn adoption_prefers_the_stage_worker_over_a_newer_adjudication_session() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");

    stage_at(&work, "alpha", StageStatus::Queued);

    let stage_session = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(&work, &stage_session);
    save_session(&stage_session, &work).unwrap();

    let mut adjudication = Session::new_adjudication("alpha");
    adjudication.status = SessionStatus::Running;
    adjudication.created_at = stage_session.created_at + chrono::Duration::seconds(60);
    spawn_a_live_agent(&work, &adjudication);
    save_session(&adjudication, &work).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.start_stage("alpha").unwrap();

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(
        after.session.as_deref(),
        Some(stage_session.id.as_str()),
        "the stage's own worker must be adopted, not the newer adjudication session"
    );
    assert_eq!(
        orchestrator
            .active_sessions
            .get("alpha")
            .map(|s| s.id.as_str()),
        Some(stage_session.id.as_str())
    );
}

/// A different session already tracked in memory than the newest live
/// session found on disk must escalate, not silently pick a side — two
/// agents may be working the same worktree.
#[test]
fn adoption_refuses_when_a_different_session_is_already_tracked() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");

    stage_at(&work, "alpha", StageStatus::Queued);

    let stage_session = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(&work, &stage_session);
    save_session(&stage_session, &work).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    let other = session_for("alpha", SessionStatus::Running);
    orchestrator
        .active_sessions
        .insert("alpha".to_string(), other.clone());

    orchestrator.start_stage("alpha").unwrap();

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.status, StageStatus::Blocked);
    assert_eq!(
        after.failure_info.map(|f| f.failure_type),
        Some(FailureType::InfrastructureError)
    );
    assert!(after.session.is_none());
    assert_eq!(
        orchestrator
            .active_sessions
            .get("alpha")
            .map(|s| s.id.as_str()),
        Some(other.id.as_str()),
        "the already-tracked session must survive the refused adoption"
    );
}

/// The write-ahead invariant's failure side: every failure between the
/// write-ahead `save_session` and a successful spawn must undo it, leaving no
/// session record and no `stage.session` link — never a stage pointing at a
/// session for an agent that never started. `block_and_undo_session` is the
/// single helper every such failure path calls, so exercising it directly
/// proves the contract every one of those call sites relies on.
#[test]
fn block_and_undo_session_leaves_no_session_record_and_no_stage_link() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");

    stage_at(&work, "alpha", StageStatus::Executing);
    let mut session = Session::new();
    session.assign_to_stage("alpha".to_string());
    save_session(&session, &work).unwrap();
    update_stage("alpha", &work, |current| {
        current.assign_session(session.id.clone());
        Ok(())
    })
    .unwrap();
    // Sanity: the write-ahead is really there before we undo it.
    assert!(work
        .join("sessions")
        .join(format!("{}.md", session.id))
        .exists());

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.block_and_undo_session(
        "alpha",
        &session.id,
        FailureType::InfrastructureError,
        "simulated pre-spawn failure".to_string(),
    );

    let after = load_stage("alpha", &work).unwrap();
    assert_eq!(after.status, StageStatus::Blocked);
    assert!(
        after.session.is_none(),
        "the stage must not still name a session that no longer exists on disk"
    );
    assert!(
        !work
            .join("sessions")
            .join(format!("{}.md", session.id))
            .exists(),
        "the write-ahead session record must be removed"
    );
}

/// The eviction guard: once a stage has a tracked session, a second insert
/// for the same stage must not silently replace it. Silent replacement is how
/// the daemon stopped monitoring an original session the moment a duplicate
/// spawned into the same worktree.
#[test]
fn insert_active_session_refuses_to_evict_an_existing_entry() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");
    let mut orchestrator = orchestrator_for(&work, temp.path());

    let incumbent = session_for("alpha", SessionStatus::Running);
    let incumbent_id = incumbent.id.clone();
    orchestrator.insert_active_session("alpha", incumbent);

    let challenger = session_for("alpha", SessionStatus::Running);
    orchestrator.insert_active_session("alpha", challenger);

    let kept = orchestrator
        .active_sessions
        .get("alpha")
        .expect("the incumbent must still be tracked");
    assert_eq!(
        kept.id, incumbent_id,
        "the first-tracked session must survive a second insert for the same stage"
    );
}
