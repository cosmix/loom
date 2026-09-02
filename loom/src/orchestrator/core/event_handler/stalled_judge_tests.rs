//! Closing a judge that stalled, and leaving everything else alone.
//!
//! The kill is observed through a real process, the same way
//! `verdict_apply_tests.rs` proves the retired judge goes down: every proxy
//! for "the session was closed" lies in at least one lane, so only the process
//! being gone, plus the persisted status, counts.

use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{
    Session, SessionBackendKind, SessionStatus, SessionType, TerminalConfig,
};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::monitor::heartbeat::judge_heartbeat_path;
use crate::orchestrator::terminal::native::{session_settings_path, write_test_pid_identity};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{create_stage, load_stage, update_stage};
use tempfile::TempDir;

/// A `.work` whose configured terminal lane is tmux, so `Orchestrator::new`
/// never runs real terminal detection (which fails on a headless test runner).
fn work_root() -> TempDir {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".work");
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

fn orchestrator_for(work_dir: &std::path::Path, repo_root: &std::path::Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap()
}

/// Start a process that outlives its parent shell, so a test can watch loom
/// actually kill it.
fn spawn_orphan_process() -> u32 {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 30 >/dev/null 2>&1 & echo $!")
        .output()
        .expect("failed to spawn a stand-in judge process");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("the stand-in judge process printed no pid")
}

/// A stage under adjudication, as `try_request_adjudication` leaves it.
fn stage_needing_adjudication(work: &std::path::Path) {
    let stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    create_stage(&stage, work).unwrap();
    update_stage("test-stage", work, |s| s.try_request_adjudication(None)).unwrap();
}

/// The signal file the daemon wrote when it spawned the session.
fn write_signal(work: &std::path::Path, session_id: &str) -> std::path::PathBuf {
    let path = work.join("signals").join(format!("{session_id}.md"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# Assignment\n").unwrap();
    path
}

/// A saved, live session of `kind` with a real process behind it.
fn live_session(work: &std::path::Path, kind: SessionType) -> (Session, u32) {
    let mut session = match kind {
        SessionType::Adjudication => Session::new_adjudication("test-stage"),
        _ => {
            let mut session = Session::new();
            session.stage_id = Some("test-stage".to_string());
            session
        }
    };
    session.status = SessionStatus::Running;
    session.backend = SessionBackendKind::Tmux;
    save_session(&session, work).unwrap();
    let pid = spawn_orphan_process();
    write_test_pid_identity(work, &session, pid).unwrap();
    assert!(crate::process::is_process_alive(pid));
    (session, pid)
}

/// A judge that stopped working is closed outright, and the stage is left
/// exactly where it is. Re-judging is `job_for_dispute`'s decision on the next
/// poll, bounded by the dispute's own attempt budget; moving the stage here
/// would take that decision away from it.
#[test]
fn a_stalled_judge_is_closed_and_the_stage_stays_under_adjudication() {
    let temp = work_root();
    let work = temp.path().join(".work");
    stage_needing_adjudication(&work);
    let (judge, judge_pid) = live_session(&work, SessionType::Adjudication);
    let signal = write_signal(&work, &judge.id);
    let heartbeat = judge_heartbeat_path(&work, "test-stage");
    std::fs::create_dir_all(heartbeat.parent().unwrap()).unwrap();
    std::fs::write(&heartbeat, "{}").unwrap();
    let capsule = session_settings_path(&work, &judge.id);
    std::fs::create_dir_all(capsule.parent().unwrap()).unwrap();
    std::fs::write(&capsule, "{}").unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .on_adjudicator_stalled(&judge.id, "test-stage", 1_000, 900)
        .unwrap();

    assert!(
        !crate::process::is_process_alive(judge_pid),
        "a judge that stopped working must be killed"
    );
    let recorded = load_session_exact(&work, &judge.id)
        .unwrap()
        .expect("the judge's session record must survive being closed");
    assert_eq!(recorded.status, SessionStatus::Crashed);
    assert!(!signal.exists(), "the judge's signal file must be removed");
    assert!(
        !heartbeat.exists(),
        "the judge's heartbeat must be removed, or it outlives the judge"
    );
    assert!(
        !capsule.exists(),
        "the judge's generated settings capsule must be removed, or it outlives the judge"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsAdjudication,
        "the stage must be left for the next poll to re-judge"
    );
}

/// The report names a session id and nothing else, so the handler re-reads the
/// record. A stage agent that happens to be named must never be taken down by
/// the judge watchdog.
#[test]
fn a_session_that_is_not_an_adjudicator_is_ignored() {
    let temp = work_root();
    let work = temp.path().join(".work");
    stage_needing_adjudication(&work);
    let (agent, agent_pid) = live_session(&work, SessionType::Stage);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .on_adjudicator_stalled(&agent.id, "test-stage", 1_000, 900)
        .unwrap();

    assert!(
        crate::process::is_process_alive(agent_pid),
        "a stage agent must not be closed by the judge watchdog"
    );
    let untouched = load_session_exact(&work, &agent.id).unwrap().unwrap();
    assert_eq!(untouched.status, SessionStatus::Running);

    let _ = crate::process::terminate(agent_pid);
}

/// A judge already closed by the verdict path is reported stalled by a poll
/// that observed it a tick earlier. Re-reading the record is what makes that
/// report harmless.
#[test]
fn a_judge_that_is_no_longer_running_is_ignored() {
    let temp = work_root();
    let work = temp.path().join(".work");
    stage_needing_adjudication(&work);
    let (mut judge, judge_pid) = live_session(&work, SessionType::Adjudication);
    judge.status = SessionStatus::Completed;
    save_session(&judge, &work).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator
        .on_adjudicator_stalled(&judge.id, "test-stage", 1_000, 900)
        .unwrap();

    let untouched = load_session_exact(&work, &judge.id).unwrap().unwrap();
    assert_eq!(untouched.status, SessionStatus::Completed);

    let _ = crate::process::terminate(judge_pid);
}
