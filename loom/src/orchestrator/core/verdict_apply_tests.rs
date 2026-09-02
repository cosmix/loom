//! Tests for retiring the adjudication session that wrote a just-applied
//! verdict.
//!
//! The kill is observed through a real process, the same way
//! `verdict_retirement_tests.rs` proves the disputing agent goes down: every
//! proxy for "the session was closed" lies in at least one lane, so only the
//! process being gone (plus the persisted session status) counts.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::dispute::{
    applied_marker, request_file, verdict_file, DisputeRequest, DisputeVerdict,
    DisputeVerdictRecord,
};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{create_stage, load_stage, update_stage};
use tempfile::TempDir;

/// A `.work` whose configured terminal lane is tmux, so `Orchestrator::new`
/// never runs real terminal detection (which fails on a headless test runner).
fn handoff_work_dir() -> TempDir {
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
/// actually kill it. `sh` exits once it has echoed the pid, reparenting
/// `sleep` to init, which reaps it as soon as it dies — a direct child would
/// instead linger as an unreaped zombie that still answers `kill(pid, 0)`.
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

fn write_dispute_request(work: &std::path::Path, stage_id: &str, id: u32) {
    let disputes_root = work.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join(id.to_string())).unwrap();
    let req = DisputeRequest {
        id,
        stage_id: stage_id.to_string(),
        criterion_index: 0,
        reason: "criterion impossible".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 1,
        created_at: chrono::Utc::now(),
    };
    let yaml = serde_yaml::to_string(&req).unwrap();
    std::fs::write(
        request_file(&disputes_root, stage_id, id),
        format!("---\n{yaml}---\n\n# Dispute {stage_id}/{id}\n"),
    )
    .unwrap();
}

/// A `NeedsMoreEvidence` verdict record — the one shape whose apply never
/// touches the plan, which keeps these fixtures focused on retirement.
fn write_verdict(work: &std::path::Path, stage_id: &str, id: u32, session_id: Option<String>) {
    let disputes_root = work.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join(id.to_string())).unwrap();
    let record = DisputeVerdictRecord {
        id,
        stage_id: stage_id.to_string(),
        verdict: DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".to_string()],
        },
        adjudicator_attempt_count: 1,
        created_at: chrono::Utc::now(),
        model: "test".to_string(),
        session_id,
    };
    let yaml = serde_yaml::to_string(&record).unwrap();
    std::fs::write(
        verdict_file(&disputes_root, stage_id, id),
        format!("---\n{yaml}---\n\n# Verdict {stage_id}/{id}\n"),
    )
    .unwrap();
}

/// A saved, live adjudication session with a real process behind it.
fn live_judge(work: &std::path::Path) -> (Session, u32) {
    let mut session = Session::new_adjudication("test-stage");
    session.status = SessionStatus::Running;
    session.backend = SessionBackendKind::Tmux;
    save_session(&session, work).unwrap();
    let pid = spawn_orphan_process();
    write_test_pid_identity(work, &session, pid).unwrap();
    assert!(crate::process::is_process_alive(pid));
    (session, pid)
}

/// The daemon must close the judge named by the applied verdict: a Claude
/// Code session does not exit on its own once its turn ends, and an idle one
/// left alive blocks the next dispute on the stage (`claim_session_slot`).
#[test]
fn applying_a_verdict_closes_the_judge_that_wrote_it() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    stage_needing_adjudication(&work);
    write_dispute_request(&work, "test-stage", 1);
    let (judge, judge_pid) = live_judge(&work);
    write_verdict(&work, "test-stage", 1, Some(judge.id.clone()));

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.apply_pending_verdicts().unwrap();

    assert!(
        !crate::process::is_process_alive(judge_pid),
        "the judge that wrote the applied verdict must be closed"
    );
    let recorded = crate::fs::session_files::load_session_exact(&work, &judge.id)
        .unwrap()
        .expect("the judge's session record must survive its own retirement");
    assert_eq!(recorded.status, SessionStatus::Completed);
    assert!(applied_marker(&work.join("disputes"), "test-stage", 1).exists());
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
}

/// A verdict names exactly one session; a second live adjudication session
/// for the same stage — however it came to exist — must never be touched.
#[test]
fn a_judge_for_another_dispute_is_left_alone() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    stage_needing_adjudication(&work);
    write_dispute_request(&work, "test-stage", 1);
    let (judge, judge_pid) = live_judge(&work);
    let (other_judge, other_pid) = live_judge(&work);
    write_verdict(&work, "test-stage", 1, Some(judge.id.clone()));

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.apply_pending_verdicts().unwrap();

    assert!(!crate::process::is_process_alive(judge_pid));
    assert!(
        crate::process::is_process_alive(other_pid),
        "a session the verdict did not name must not be closed"
    );
    let untouched = crate::fs::session_files::load_session_exact(&work, &other_judge.id)
        .unwrap()
        .unwrap();
    assert_eq!(untouched.status, SessionStatus::Running);

    let _ = crate::process::terminate(other_pid);
}

/// A verdict written before `session_id` existed falls back to closing any
/// live judge for the stage — but only once no dispute is left unanswered,
/// since a live judge might still be working one of those.
#[test]
fn a_verdict_without_a_session_id_closes_idle_judges_only_when_no_dispute_remains() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    stage_needing_adjudication(&work);
    write_dispute_request(&work, "test-stage", 1);
    write_dispute_request(&work, "test-stage", 2);
    let (judge, judge_pid) = live_judge(&work);
    write_verdict(&work, "test-stage", 1, None);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.apply_pending_verdicts().unwrap();

    assert!(
        crate::process::is_process_alive(judge_pid),
        "dispute 2 is still unanswered; the judge must stay alive"
    );

    write_verdict(&work, "test-stage", 2, None);
    orchestrator.apply_pending_verdicts().unwrap();

    assert!(
        !crate::process::is_process_alive(judge_pid),
        "the last dispute was just answered; the idle judge must now close"
    );
    let recorded = crate::fs::session_files::load_session_exact(&work, &judge.id)
        .unwrap()
        .unwrap();
    assert_eq!(recorded.status, SessionStatus::Completed);
}
