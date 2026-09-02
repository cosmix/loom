//! Unit tests for [`super`], the "is an agent already running for this stage?"
//! registry.
//!
//! Every fixture here builds liveness the way the spawn path does — a PID
//! identity file under `.loom/work/pids/` naming the TEST's own process — because
//! that file, not the session record, is what survives a daemon death and is
//! the only evidence adoption has to work from.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::TerminalConfig;
use crate::models::stage::Stage;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::orchestrator::terminal::tmux::viewer::live_tmux_sessions;
use crate::verify::transitions::save_stage;
use tempfile::TempDir;

/// A `.loom/work` directory whose configured terminal lane is tmux.
///
/// The tmux setting is about the TEST HOST, not the sessions under test:
/// `SessionBackend::from_config` eagerly constructs a `NativeBackend` when the
/// configured lane is native, and that runs terminal detection, which fails on
/// a headless runner. Configuring tmux defers the native lane to
/// `native_lane()`, which degrades to `pid_only_is_alive` — the same verdict a
/// terminal-having host reaches through `NativeBackend::is_session_alive`,
/// which consults PID identity before it ever looks at a window.
fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    temp
}

fn session_for(stage_id: &str, status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = status;
    session
}

/// The PID file the wrapper script writes at spawn, naming this process so the
/// identity probe answers `VerifiedAlive`.
fn spawn_a_live_agent(work: &Path, session: &Session) {
    write_test_pid_identity(work, session, std::process::id()).unwrap();
}

/// A PID file for a process that is definitively gone: a PID that cannot exist
/// plus a start-time that could never match it.
fn spawn_a_dead_agent(work: &Path, session: &Session) {
    std::fs::create_dir_all(work.join("pids")).unwrap();
    let pid_key = format!("{}-{}", session.tracking_key, session.id);
    std::fs::write(
        work.join("pids").join(format!("{pid_key}.pid")),
        "999999999\n1\n",
    )
    .unwrap();
}

fn stage_at(work: &Path, stage_id: &str, status: StageStatus) {
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = status;
    save_stage(&stage, work).unwrap();
}

fn ids(sessions: &[Session]) -> Vec<&str> {
    sessions.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn live_sessions_for_stage_lists_only_live_records_of_that_stage() {
    let temp = work_dir();
    let work = temp.path();

    let alive = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &alive);
    save_session(&alive, work).unwrap();

    // Live, but working for a different stage.
    let elsewhere = session_for("beta", SessionStatus::Running);
    spawn_a_live_agent(work, &elsewhere);
    save_session(&elsewhere, work).unwrap();

    // Right stage, live process, but the record says it is finished. Status
    // filtering has to come first or a stage would look busy forever.
    let finished = session_for("alpha", SessionStatus::Completed);
    spawn_a_live_agent(work, &finished);
    save_session(&finished, work).unwrap();

    // Right stage, `Running` on paper, process gone.
    let dead = session_for("alpha", SessionStatus::Running);
    spawn_a_dead_agent(work, &dead);
    save_session(&dead, work).unwrap();

    // A file caught mid-write must not fail the scan and report the stage idle.
    std::fs::write(
        work.join("sessions").join("session-corrupt-1.md"),
        "this is not session frontmatter",
    )
    .unwrap();

    let live = live_sessions_for_stage(work, "alpha").unwrap();
    assert_eq!(ids(&live), vec![alive.id.as_str()]);
}

#[test]
fn in_progress_sessions_for_stage_keeps_dead_records_for_takedown() {
    let temp = work_dir();
    let work = temp.path();

    let live = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &live);
    save_session(&live, work).unwrap();

    // This is the record that outlives a daemon restart after its process has
    // already exited. It is not live enough to block spawning, but takedown
    // still needs it so it can declare the deliberate handoff terminal.
    let dead = session_for("alpha", SessionStatus::Running);
    spawn_a_dead_agent(work, &dead);
    save_session(&dead, work).unwrap();

    let spawning = session_for("alpha", SessionStatus::Spawning);
    save_session(&spawning, work).unwrap();

    let completed = session_for("alpha", SessionStatus::Completed);
    save_session(&completed, work).unwrap();

    let mut found = in_progress_sessions_for_stage(work, "alpha")
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect::<Vec<_>>();
    let mut expected = vec![live.id, dead.id, spawning.id];
    found.sort();
    expected.sort();
    assert_eq!(found, expected);
}

#[test]
fn in_progress_scan_rejects_filename_and_record_identity_mismatch() {
    let temp = work_dir();
    let work = temp.path();
    let session = session_for("alpha", SessionStatus::Running);
    let sessions_dir = work.join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::write(
        sessions_dir.join("actual.md"),
        crate::fs::session_files::session_to_markdown(&session),
    )
    .unwrap();

    let error = in_progress_sessions_for_stage(work, "alpha").unwrap_err();

    assert!(format!("{error:#}").contains("does not match record id"));
}

/// A `.loom/work` directory configured for the native lane — the setting that makes
/// `SessionBackend::from_config` build a `NativeBackend` eagerly.
fn native_work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Native,
        },
    )
    .unwrap();
    temp
}

#[test]
fn a_native_configured_work_dir_never_makes_the_answer_an_error() {
    // `loom stage reset` and the spawn guard in `start_stage` both call this
    // unconditionally, so an `Err` here would stop resets working and stop the
    // daemon spawning ANY stage. On a host with a terminal this runs the
    // backend branch; on a headless one (CI, a container) it runs the PID-only
    // fallback. Neither may error, and both must see the agent.
    let temp = native_work_dir();
    let work = temp.path();

    let alive = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &alive);
    save_session(&alive, work).unwrap();

    let live = live_sessions_for_stage(work, "alpha").unwrap();
    assert_eq!(ids(&live), vec![alive.id.as_str()]);
}

#[test]
fn the_pid_only_probe_answers_liveness_with_no_terminal_backend_at_all() {
    // Drives the fallback branch DIRECTLY. Forcing `detect_terminal` to fail
    // is not possible from a test: it falls through `LOOM_TERMINAL`, then
    // `TERMINAL`, then gsettings, xdg-terminal-exec and every common emulator,
    // so on any developer machine it succeeds whatever the environment says.
    // Asserting through `live_sessions_for_stage` alone would therefore leave
    // this branch untested exactly where it is not exercised anyway.
    let temp = native_work_dir();
    let work = temp.path();

    let alive = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &alive);
    save_session(&alive, work).unwrap();

    let dead = session_for("alpha", SessionStatus::Running);
    spawn_a_dead_agent(work, &dead);
    save_session(&dead, work).unwrap();

    let live = live_sessions_with_probe(&LivenessProbe::PidOnly, work, "alpha");
    assert_eq!(
        ids(&live),
        vec![alive.id.as_str()],
        "PID identity alone must still tell a live agent from a dead one"
    );
}

#[test]
fn live_sessions_for_stage_is_empty_before_any_session_exists() {
    let temp = work_dir();
    assert!(live_sessions_for_stage(temp.path(), "alpha")
        .unwrap()
        .is_empty());
}

/// An adjudication session carries the stage's own `stage_id`, so a scan
/// keyed on the stage alone cannot tell it apart from the stage's worker.
/// Adoption of the worker slot must use the typed lookup, not the untyped
/// one status rendering and attach still rely on.
#[test]
fn typed_lookup_excludes_a_live_adjudication_session() {
    let temp = work_dir();
    let work = temp.path();

    let worker = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &worker);
    save_session(&worker, work).unwrap();

    let mut adjudication = Session::new_adjudication("alpha");
    adjudication.status = SessionStatus::Running;
    spawn_a_live_agent(work, &adjudication);
    save_session(&adjudication, work).unwrap();

    let typed = live_sessions_for_stage_of_type(work, "alpha", SessionType::Stage).unwrap();
    assert_eq!(ids(&typed), vec![worker.id.as_str()]);

    let untyped = live_sessions_for_stage(work, "alpha").unwrap();
    assert_eq!(untyped.len(), 2);
}

#[path = "tests_session_registry_adoption.rs"]
mod adoption_tests;
