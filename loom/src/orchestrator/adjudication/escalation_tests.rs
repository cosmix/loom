//! The dispute loop's escalations, driven through
//! `Orchestrator::check_pending_disputes`: each one retires the stage's
//! disputing agent before `NeedsHumanReview` is written, so approving the
//! review can never re-queue the stage onto the idle agent.
//!
//! The kill is observed through a real process, as in
//! `orchestrator/core/event_handler/verdict_retirement_tests.rs`: every proxy
//! for "the agent was taken down" lies in at least one lane.

use crate::process::sandbox_probe::skip_without_group_signals;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::tests::{make_stage, write_dispute_request, write_stage};
use super::MAX_EVIDENCE_ROUNDS;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::handoff::generator::find_latest_handoff;
use crate::handoff::schema::{HandoffOrigin, ParsedHandoff};
use crate::models::session::{
    Session, SessionBackendKind, SessionExitReason, SessionStatus, TerminalConfig,
};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::orchestrator::{Orchestrator, OrchestratorConfig};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::load_stage;

/// A repository whose `.loom/work` configures the tmux lane, so
/// `Orchestrator::new` never runs real terminal detection.
fn work_dir() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_terminal_config(
        &work,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    (temp, work)
}

fn orchestrator_for(work: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap()
}

/// Start a process that outlives its parent shell, so the test can watch the
/// retirement kill it. `sh` exits once it has echoed the pid, reparenting the
/// `sleep` to init, which reaps it as soon as it dies; a direct child would
/// linger as a zombie that still answers `kill(pid, 0)`.
fn spawn_orphan_process() -> u32 {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 30 >/dev/null 2>&1 & echo $!")
        .output()
        .expect("failed to spawn a stand-in agent process");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("the stand-in agent process printed no pid")
}

/// A running worker record for `s1`, with no PID identity yet.
fn worker_session(work: &Path) -> Session {
    let mut worker = Session::new();
    worker.assign_to_stage("s1".to_string());
    worker.status = SessionStatus::Running;
    worker.backend = SessionBackendKind::Tmux;
    save_session(&worker, work).unwrap();
    worker
}

/// Save `stage` as named by `worker`, with one unanswered dispute: filing a
/// dispute does not end the filing session, so the stage still names it.
fn file_dispute(work: &Path, mut stage: Stage, worker: &Session) {
    stage.assign_session(worker.id.clone());
    write_stage(work, &stage);
    write_dispute_request(work, &stage.id, 1, 0);
}

/// A `NeedsAdjudication` stage whose disputing agent is a live process.
fn live_disputing_stage(work: &Path, stage: Stage) -> (Session, u32) {
    let worker = worker_session(work);
    let pid = spawn_orphan_process();
    write_test_pid_identity(work, &worker, pid).unwrap();
    assert!(crate::process::is_process_alive(pid));
    file_dispute(work, stage, &worker);
    (worker, pid)
}

fn assert_retired_then_escalated(work: &Path, worker: &Session, pid: u32) {
    let stage = load_stage("s1", work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(
        stage.session, None,
        "approving the review must not find the idle agent still assigned"
    );
    assert!(
        !crate::process::is_process_alive(pid),
        "the disputing agent must be killed before the stage is escalated"
    );
    let retired = load_session_exact(work, &worker.id).unwrap().unwrap();
    assert_eq!(
        (retired.status, retired.exit_reason),
        (
            SessionStatus::ContextExhausted,
            Some(SessionExitReason::Replaced)
        )
    );
    let handoff = find_latest_handoff("s1", work)
        .unwrap()
        .expect("the successor needs the retired agent's handoff");
    let content = std::fs::read_to_string(handoff).unwrap();
    assert_eq!(
        ParsedHandoff::parse(&content)
            .as_v2()
            .and_then(|handoff| handoff.origin),
        Some(HandoffOrigin::Retired)
    );
}

/// The incident this pins: the adjudicator could not be spawned, the stage
/// went to `NeedsHumanReview` with its disputing agent still live and
/// assigned, and approving the review let the executor adopt that agent,
/// silently abandoning the dispute.
#[test]
fn a_failed_adjudicator_spawn_retires_the_disputing_agent_before_escalating() {
    if skip_without_group_signals(
        "a_failed_adjudicator_spawn_retires_the_disputing_agent_before_escalating",
    ) {
        return;
    }
    let (temp, work) = work_dir();
    let (worker, pid) = live_disputing_stage(&work, make_stage("s1"));
    // Every adjudication spawn fails before reaching a terminal backend: the
    // judge's signal cannot be written under a `signals` that is a file.
    std::fs::write(work.join("signals"), b"not a directory").unwrap();

    orchestrator_for(&work, temp.path())
        .check_pending_disputes()
        .unwrap();

    assert_retired_then_escalated(&work, &worker, pid);
    let stage = load_stage("s1", &work).unwrap();
    assert!(stage
        .review_reason
        .as_deref()
        .unwrap_or("")
        .contains("No adjudication session could be started"));
    assert!(
        stage.failure_info.is_some(),
        "a spawn failure records its classification"
    );
}

#[test]
fn the_evidence_cap_retires_the_disputing_agent_before_escalating() {
    if skip_without_group_signals("the_evidence_cap_retires_the_disputing_agent_before_escalating")
    {
        return;
    }
    let (temp, work) = work_dir();
    let mut stage = make_stage("s1");
    stage.tally.evidence_rounds = MAX_EVIDENCE_ROUNDS;
    let (worker, pid) = live_disputing_stage(&work, stage);

    orchestrator_for(&work, temp.path())
        .check_pending_disputes()
        .unwrap();

    assert_retired_then_escalated(&work, &worker, pid);
}

/// An agent the takedown cannot prove dead does not hold the escalation back:
/// the stage still goes to a human, with the surviving session and the
/// command that takes it down named in the review reason. Approval refuses
/// while that session is live (`commands/stage/human_review.rs`).
#[test]
fn an_agent_that_survives_retirement_is_named_in_the_escalation() {
    let (temp, work) = work_dir();
    let mut stage = make_stage("s1");
    stage.tally.evidence_rounds = MAX_EVIDENCE_ROUNDS;
    // No PID identity at all: the takedown refuses to call that death.
    let worker = worker_session(&work);
    file_dispute(&work, stage, &worker);

    orchestrator_for(&work, temp.path())
        .check_pending_disputes()
        .unwrap();

    let after = load_stage("s1", &work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    assert_eq!(after.session.as_deref(), Some(worker.id.as_str()));
    let reason = after.review_reason.unwrap_or_default();
    assert!(reason.starts_with("Evidence loop exhausted"), "{reason}");
    assert!(reason.contains(&worker.id), "{reason}");
    assert!(
        reason.contains("loom stage reset s1 --kill-session"),
        "{reason}"
    );
}
