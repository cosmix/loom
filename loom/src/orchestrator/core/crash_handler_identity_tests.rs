//! A crash may only speak for the stage's CURRENT session.
//!
//! Session files accumulate: a stage that crashed and retried leaves every
//! previous session on disk with `stage_id` still pointing at it. `Orchestrator
//! ::reported_crashes` is in-memory, so a daemon restart re-observes all of
//! them as new. Without an identity check those replays are charged to the
//! stage's retry budget and can auto-retry a stage whose real session is alive
//! and working — two agents in one worktree.

use super::Orchestrator;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage};
use serial_test::serial;
use std::path::Path;

/// `Orchestrator::new` eagerly constructs a `NativeBackend`, so it fails on a
/// headless runner with no terminal emulator installed. Pinning
/// `LOOM_TERMINAL` maps a name straight to an emulator without probing the
/// host. Serialized because the detection tests mutate the same process-global.
fn pin_terminal_env() -> Option<std::ffi::OsString> {
    let saved = std::env::var_os("LOOM_TERMINAL");
    // SAFETY: the test is serialized and restores the original value below.
    unsafe { std::env::set_var("LOOM_TERMINAL", "xterm") };
    saved
}

fn restore_terminal_env(saved: Option<std::ffi::OsString>) {
    match saved {
        // SAFETY: the serialized test is restoring its saved value.
        Some(value) => unsafe { std::env::set_var("LOOM_TERMINAL", value) },
        // SAFETY: the serialized test is restoring the variable's absence.
        None => unsafe { std::env::remove_var("LOOM_TERMINAL") },
    }
}

/// An `Executing` stage whose active session is `active_session`.
fn executing_stage(work_dir: &Path, active_session: &str) -> Stage {
    let mut stage = Stage::new("weather-cache".to_string(), None);
    stage.id = "weather-cache".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some(active_session.to_string());
    save_stage(&stage, work_dir).unwrap();
    stage
}

fn orchestrator_for(work_dir: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let saved = pin_terminal_env();
    let constructed = Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap());
    restore_terminal_env(saved);
    constructed.unwrap()
}

/// THE REGRESSION THIS PINS (observed live 2026-08-10): a session dead for 25
/// minutes was replayed on daemon restart, blocked a healthy `Executing` stage,
/// and auto-retried it — spawning a second agent into a worktree whose first
/// agent was still writing to it.
#[test]
#[serial]
fn a_stale_session_crash_cannot_block_a_stage_running_under_another_session() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".work");
    let stage = executing_stage(&work_dir, "session-live");
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());

    orchestrator
        .handle_session_crashed("session-corpse", Some(stage.id.clone()), None)
        .unwrap();

    let after = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(
        after.status,
        StageStatus::Executing,
        "a corpse from an earlier attempt must not move a stage executing under a live session"
    );
    assert_eq!(
        after.session.as_deref(),
        Some("session-live"),
        "the stage's active session must be untouched"
    );
}

/// The mirror: the stage's OWN session crashing must still act, or a stage
/// stranded by a daemon that died between the crash and handling it would sit
/// `Executing` forever.
#[test]
#[serial]
fn the_stages_own_session_crashing_still_moves_the_stage() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".work");
    let stage = executing_stage(&work_dir, "session-live");
    let mut session = Session::new();
    session.id = "session-live".to_string();
    session.stage_id = Some(stage.id.clone());
    session.status = SessionStatus::Crashed;
    crate::fs::session_files::save_session(&session, &work_dir).unwrap();
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());

    orchestrator
        .handle_session_crashed("session-live", Some(stage.id.clone()), None)
        .unwrap();

    let after = load_stage(&stage.id, &work_dir).unwrap();
    assert_ne!(
        after.status,
        StageStatus::Executing,
        "a crash of the stage's own session must move it out of Executing"
    );
}
