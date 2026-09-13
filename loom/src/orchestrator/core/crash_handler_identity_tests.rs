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
    let work_dir = temp.path().join(".loom").join("work");
    let stage = executing_stage(&work_dir, "session-live");
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());
    let mut live = Session::new();
    live.id = "session-live".to_string();
    live.stage_id = Some(stage.id.clone());
    live.status = SessionStatus::Running;
    orchestrator.active_sessions.insert(stage.id.clone(), live);

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
    assert_eq!(
        orchestrator
            .active_sessions
            .get(&stage.id)
            .map(|session| session.id.as_str()),
        Some("session-live"),
        "a stale predecessor crash must not drop the healthy successor's in-memory handle"
    );
}

/// The mirror: the stage's OWN session crashing must still act, or a stage
/// stranded by a daemon that died between the crash and handling it would sit
/// `Executing` forever.
#[test]
#[serial]
fn the_stages_own_session_crashing_still_moves_the_stage() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
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

/// Recursively list every regular file under `dir`, relative to `dir`.
fn files_under(dir: &Path) -> std::collections::BTreeSet<std::path::PathBuf> {
    let mut found = std::collections::BTreeSet::new();
    fn walk(base: &Path, dir: &Path, found: &mut std::collections::BTreeSet<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, found);
            } else {
                found.insert(path.strip_prefix(base).unwrap().to_path_buf());
            }
        }
    }
    walk(dir, dir, &mut found);
    found
}

/// Before this fix, a fast crash while Remote Control looked active wrote a
/// marker file so the NEXT spawn's arguments silently changed. That marker is
/// gone: a fast, verified-pid crash is handled like any other crash and
/// writes nothing new under the work dir — no crash report is supplied here,
/// so nothing at all should appear.
#[test]
#[serial]
fn a_fast_fail_crash_writes_no_file_under_the_work_dir() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    let stage = executing_stage(&work_dir, "session-fast");
    let mut session = Session::new();
    session.id = "session-fast".to_string();
    session.stage_id = Some(stage.id.clone());
    session.status = SessionStatus::Running;
    session.pid = Some(4242);
    session.created_at = chrono::Utc::now();
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());
    orchestrator
        .active_sessions
        .insert(stage.id.clone(), session);

    let before = files_under(&work_dir);

    orchestrator
        .handle_session_crashed("session-fast", Some(stage.id.clone()), None)
        .unwrap();

    let after = files_under(&work_dir);
    let new_files: Vec<_> = after.difference(&before).collect();
    assert!(
        new_files.is_empty(),
        "a fast-fail crash with no crash report must write no new file under the work dir, got: {new_files:?}"
    );
}

/// A fast crash while Remote Control is suspected active must not be a dead
/// end for the operator: no marker persists it, the stage keeps its retry
/// budget instead of being blocked as a startup refusal, and the very next
/// `resolve()` call in this process reports Remote Control off.
///
/// Remote Control's own activity is injected via `Orchestrator
/// ::remote_control_active` rather than faking a `claude` install on `PATH` —
/// a test must never mutate process-wide environment (`PATH`, `HOME`, and the
/// like); see `SessionBackend::tmux_available` for the same pattern.
#[test]
#[serial]
fn a_suspected_remote_control_crash_disables_it_and_stays_retryable() {
    crate::remote_control::reset_disabled_for_process();
    let temp = tempfile::tempdir().unwrap();

    let work_dir = temp.path().join(".loom").join("work");
    let stage = executing_stage(&work_dir, "session-rc");
    let mut session = Session::new();
    session.id = "session-rc".to_string();
    session.stage_id = Some(stage.id.clone());
    session.status = SessionStatus::Running;
    session.pid = Some(4242);
    session.created_at = chrono::Utc::now();
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());
    orchestrator.remote_control_active = |_| true;
    orchestrator
        .active_sessions
        .insert(stage.id.clone(), session);

    let before = files_under(&work_dir);
    orchestrator
        .handle_session_crashed("session-rc", Some(stage.id.clone()), None)
        .unwrap();
    let after = files_under(&work_dir);
    assert_eq!(
        after.difference(&before).count(),
        0,
        "a suspected remote-control crash must write no new file"
    );

    let stage_after = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(
        stage_after.status,
        StageStatus::Blocked,
        "the crash still blocks the stage so the orchestrator's retry loop can pick it up"
    );
    assert_eq!(
        stage_after.failure_info.unwrap().failure_type,
        crate::models::failure::FailureType::SessionCrash,
        "it must classify as an ordinary crash, not a startup refusal"
    );

    assert!(
        !crate::remote_control::resolve(&work_dir),
        "resolve() must report Remote Control off for the rest of this process"
    );
    crate::remote_control::reset_disabled_for_process();
}

/// The mirror of the case above: with Remote Control read as INACTIVE, the
/// same fast, verified-pid crash is an ordinary startup refusal (blocked, not
/// retried), and it must not touch the process-global disable latch at all.
#[test]
#[serial]
fn a_fast_fail_crash_with_remote_control_inactive_is_a_startup_refusal() {
    crate::remote_control::reset_disabled_for_process();
    let temp = tempfile::tempdir().unwrap();

    let work_dir = temp.path().join(".loom").join("work");
    let stage = executing_stage(&work_dir, "session-refusal");
    let mut session = Session::new();
    session.id = "session-refusal".to_string();
    session.stage_id = Some(stage.id.clone());
    session.status = SessionStatus::Running;
    session.pid = Some(4242);
    session.created_at = chrono::Utc::now();
    let mut orchestrator = orchestrator_for(&work_dir, temp.path());
    orchestrator.remote_control_active = |_| false;
    orchestrator
        .active_sessions
        .insert(stage.id.clone(), session);

    orchestrator
        .handle_session_crashed("session-refusal", Some(stage.id.clone()), None)
        .unwrap();

    let stage_after = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(
        stage_after.status,
        StageStatus::Blocked,
        "a startup refusal still blocks the stage"
    );
    assert_eq!(
        stage_after.failure_info.unwrap().failure_type,
        crate::models::failure::FailureType::StartupRefusal,
        "with Remote Control inactive, a fast verified-pid crash is a startup refusal"
    );
    assert!(
        !crate::remote_control::disabled_for_process(),
        "a crash unrelated to Remote Control must not latch it off for the process"
    );
}
