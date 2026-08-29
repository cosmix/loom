//! Unit tests for [`super`], the "is an agent already running for this stage?"
//! registry.
//!
//! Every fixture here builds liveness the way the spawn path does — a PID
//! identity file under `.work/pids/` naming the TEST's own process — because
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

/// A `.work` directory whose configured terminal lane is tmux.
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

/// A `.work` directory configured for the native lane — the setting that makes
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

#[test]
fn orphan_evidence_finds_the_recordless_agent_and_ignores_every_recorded_one() {
    let temp = work_dir();
    let work = temp.path();

    // The orphan: `Executing`, a live agent, and no session record at all.
    stage_at(work, "alpha", StageStatus::Executing);
    let orphan = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &orphan);

    // Healthy: the record exists and is live, so the stage is already visible
    // to attach and recovery. Not an orphan even though a PID file is present.
    stage_at(work, "beta", StageStatus::Executing);
    let healthy = session_for("beta", SessionStatus::Running);
    spawn_a_live_agent(work, &healthy);
    save_session(&healthy, work).unwrap();

    // A record that exists but reports `Completed` while its process is still
    // up: a record problem for the file-driven pass to judge, not an orphan.
    // This is the case that isolates the `sessions/<id>.md` existence check
    // from the liveness check above it.
    stage_at(work, "gamma", StageStatus::Executing);
    let stale_record = session_for("gamma", SessionStatus::Completed);
    spawn_a_live_agent(work, &stale_record);
    save_session(&stale_record, work).unwrap();

    // Not `Executing`: never scanned, whatever its PID files say.
    stage_at(work, "delta", StageStatus::Queued);
    let not_executing = session_for("delta", SessionStatus::Running);
    spawn_a_live_agent(work, &not_executing);

    let evidence = orphan_evidence(work);
    assert_eq!(evidence.len(), 1, "unexpected evidence: {evidence:?}");
    assert_eq!(
        evidence[0],
        OrphanEvidence {
            session_id: orphan.id.clone(),
            stage_id: "alpha".to_string(),
            tracking_key: "loom-alpha".to_string(),
            session_type: SessionType::Stage,
            pid: std::process::id(),
            backend: SessionBackendKind::Native,
        }
    );
}

#[test]
fn a_dead_agents_pid_file_is_not_evidence_of_an_orphan() {
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let corpse = session_for("alpha", SessionStatus::Running);
    spawn_a_dead_agent(work, &corpse);

    assert!(orphan_evidence(work).is_empty());
}

#[test]
fn adoption_is_idempotent_because_its_record_hides_the_pid_file() {
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let orphan = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &orphan);

    let first = orphan_evidence(work);
    assert_eq!(first.len(), 1);
    adopt_orphan(work, &first[0]).unwrap();

    // The record written above is what the next scan trips over — the same
    // reason a healthy stage is never adopted twice by the recovery tick.
    assert!(
        orphan_evidence(work).is_empty(),
        "a second pass re-adopted an agent it had already recorded"
    );
}

#[test]
fn an_adopted_record_is_attachable_again() {
    let temp = work_dir();
    let work = temp.path();

    let session_id = "session-abcd1234-1700000000";
    let evidence = OrphanEvidence {
        session_id: session_id.to_string(),
        stage_id: "alpha".to_string(),
        tracking_key: "loom-alpha".to_string(),
        session_type: SessionType::Stage,
        pid: std::process::id(),
        backend: SessionBackendKind::Tmux,
    };

    // The PID file the dead daemon's wrapper left behind, under the exact key
    // `window_title_and_pid_key` will look the adopted session up by.
    let mut spawned = Session::new();
    spawned.id = session_id.to_string();
    spawned.assign_to_stage("alpha".to_string());
    spawn_a_live_agent(work, &spawned);

    let adopted = adopt_orphan(work, &evidence).unwrap();
    assert_eq!(adopted.id, session_id);
    assert_eq!(adopted.stage_id.as_deref(), Some("alpha"));
    assert_eq!(adopted.tracking_key, "loom-alpha");
    assert_eq!(adopted.session_type, SessionType::Stage);
    assert_eq!(adopted.status, SessionStatus::Running);
    assert_eq!(adopted.backend, SessionBackendKind::Tmux);
    assert_eq!(adopted.pid, Some(std::process::id()));

    // The point of the whole change: `loom attach`'s discovery set is
    // `live_tmux_sessions`, and it now sees the agent again.
    let attachable = live_tmux_sessions(work).unwrap();
    assert_eq!(ids(&attachable), vec![session_id]);

    // And the stage-side question answers consistently.
    let live = live_sessions_for_stage(work, "alpha").unwrap();
    assert_eq!(ids(&live), vec![session_id]);
}

#[test]
fn the_adoption_pass_links_the_stage_without_touching_its_status() {
    // The link half of `Recovery::adopt_orphaned_agents`, which cannot be
    // driven directly here: it needs a live `Orchestrator` (backend, monitor,
    // graph), the same reason `core::recovery`'s own tests exercise helpers
    // rather than the pass. What is asserted is the contract that pass relies
    // on — a stage still `Executing` and naming nobody gains the adopted
    // session, and `Executing` is left standing because it just became true
    // again.
    let temp = work_dir();
    let work = temp.path();

    stage_at(work, "alpha", StageStatus::Executing);
    let orphan = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &orphan);

    let evidence = orphan_evidence(work);
    assert_eq!(evidence.len(), 1);
    let adopted = adopt_orphan(work, &evidence[0]).unwrap();

    crate::verify::transitions::update_stage("alpha", work, |stage| {
        if stage.status == StageStatus::Executing && stage.session.is_none() {
            stage.session = Some(adopted.id.clone());
        }
        Ok(())
    })
    .unwrap();

    let stage = crate::verify::transitions::load_stage("alpha", work).unwrap();
    assert_eq!(stage.session.as_deref(), Some(adopted.id.as_str()));
    assert_eq!(stage.status, StageStatus::Executing);
}

#[test]
fn a_merge_agents_tracking_key_survives_adoption() {
    let temp = work_dir();
    let work = temp.path();

    let session_id = "session-beef0001-1700000001";
    let evidence = OrphanEvidence {
        session_id: session_id.to_string(),
        stage_id: "alpha".to_string(),
        tracking_key: "loom-merge-alpha".to_string(),
        session_type: SessionType::Merge,
        pid: std::process::id(),
        backend: SessionBackendKind::Native,
    };

    let adopted = adopt_orphan(work, &evidence).unwrap();
    // `assign_to_stage` derives a key from `(stage_id, session_type)`, so a
    // kind set after the assignment would leave a `loom-alpha` key on a merge
    // session and every PID lookup for it would miss.
    assert_eq!(adopted.tracking_key, "loom-merge-alpha");
    assert_eq!(adopted.session_type, SessionType::Merge);
}
