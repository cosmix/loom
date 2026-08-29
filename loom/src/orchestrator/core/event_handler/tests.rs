//! Tests for monitor-event handling and the handoff/backstop transitions.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{SessionBackendKind, SessionStatus, TerminalConfig};
use crate::models::stage::{Implementers, Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::plan::schema::{StageDefinition, StageSandboxConfig};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{create_stage, load_stage, update_stage};
use std::path::Path;
use tempfile::TempDir;

fn create_test_graph() -> ExecutionGraph {
    let stages = vec![StageDefinition {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: None,
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_ceiling_tokens: None,
        removed_context_budget: None,
        plan_overview: None,
        sandbox: StageSandboxConfig::default(),
        execution_mode: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        code_review: None,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    }];
    ExecutionGraph::build(stages).unwrap()
}

/// A `.work` whose configured terminal lane is tmux, so `Orchestrator::new`
/// never runs real terminal detection (which fails on a headless test runner).
/// Same trick `stage_executor_tests.rs` uses for the same reason.
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

fn orchestrator_for(work_dir: &Path, repo_root: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, create_test_graph()).unwrap()
}

/// A session record for `test-stage`, as the daemon would have left on disk
/// before it restarted.
fn recorded_session(work_dir: &Path) -> crate::models::session::Session {
    let mut session = crate::models::session::Session::new();
    session.assign_to_stage("test-stage".to_string());
    session.status = SessionStatus::Running;
    session.backend = SessionBackendKind::Tmux;
    save_session(&session, work_dir).unwrap();
    session
}

/// Write the PID file the wrapper script leaves at spawn.
///
/// `start_time` drives what the identity probe concludes, and both cases the
/// takedown must tell apart are reachable WITHOUT ever signalling this test
/// process: `None` (the wrapper wrote no start time) verifies as
/// `Unverifiable`, which counts as ALIVE and which `terminate_verified`
/// refuses to signal; a start time that cannot be this process's verifies as
/// `Dead`.
fn write_pid_file(
    work_dir: &Path,
    session: &crate::models::session::Session,
    start_time: Option<u64>,
) {
    let pids = work_dir.join("pids");
    std::fs::create_dir_all(&pids).unwrap();
    let mut contents = format!("{}\n", std::process::id());
    if let Some(start_time) = start_time {
        contents.push_str(&format!("{start_time}\n"));
    }
    let path = pids.join(format!("{}-{}.pid", session.tracking_key, session.id));
    std::fs::write(path, contents).unwrap();
}

/// Start a process that outlives its parent shell, so a test can watch loom
/// actually kill it.
///
/// `sh` exits the moment it has echoed the pid, which reparents the `sleep` to
/// init — and init reaps it as soon as it dies, so `kill(pid, 0)` reports it
/// gone. A DIRECT child of the test process would instead linger as an
/// unreaped zombie that still answers `kill(pid, 0)`, and the assertion would
/// prove nothing.
fn spawn_orphan_process() -> u32 {
    let output = std::process::Command::new("sh")
        .arg("-c")
        // The background process must not inherit the pipe `output()` reads, or
        // the read blocks until the `sleep` itself exits.
        .arg("sleep 30 >/dev/null 2>&1 & echo $!")
        .output()
        .expect("failed to spawn a stand-in agent process");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("the stand-in agent process printed no pid")
}

fn executing_stage(work_dir: &Path) {
    let stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    create_stage(&stage, work_dir).unwrap();
}

/// The daemon backstop, driven through its real entry point: it must KILL the
/// agent it hands off, not merely mark the stage and re-queue.
///
/// The kill is observed directly — a real process, signalled by the real
/// backend — because every proxy for it is unreliable: the tmux lane returns
/// `Ok` from `kill_session` unconditionally, and the native lane returns `Ok`
/// when it refuses to signal an unverifiable identity. Only the process being
/// gone proves the takedown happened.
#[test]
fn budget_backstop_kills_the_agent_and_then_requeues() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let session = recorded_session(&work);
    let agent_pid = spawn_orphan_process();
    // A full identity (pid + start time) is what makes the teardown willing to
    // signal: `pid_only_terminate` refuses anything less.
    write_test_pid_identity(&work, &session, agent_pid).unwrap();
    assert!(crate::process::is_process_alive(agent_pid));

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .handle_budget_exceeded(&session.id, "test-stage", 200_000, 150_000)
        .unwrap();

    assert!(
        !crate::process::is_process_alive(agent_pid),
        "the backstop must kill the agent it hands off"
    );
    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
    assert!(graph_has_ready_stage(&orchestrator.graph, "test-stage"));
    assert!(orchestrator.active_sessions.is_empty());
    // The handoff is written before the kill, while the record still describes
    // a running agent.
    assert!(work
        .join("handoffs")
        .join("test-stage-handoff-001.md")
        .exists());
}

/// The other direction, through the same entry point: when the agent survives
/// the kill the stage must NOT be re-queued, and the daemon must keep its
/// handle on the session so the takedown can be retried.
#[test]
fn budget_backstop_does_not_requeue_an_agent_it_could_not_kill() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let session = recorded_session(&work);
    // PID identity with no start time: alive, and unkillable through loom.
    write_pid_file(&work, &session, None);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    orchestrator
        .active_sessions
        .insert("test-stage".to_string(), session.clone());

    orchestrator
        .handle_budget_exceeded(&session.id, "test-stage", 200_000, 150_000)
        .unwrap();

    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff,
        "a stage whose agent survived the kill must not go back to Queued"
    );
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));
    assert!(
        orchestrator.active_sessions.contains_key("test-stage"),
        "the surviving session must stay tracked, or the next attempt has \
         nothing to find"
    );
}

/// The double-spawn this guard exists to stop: `active_sessions` is in-memory
/// only and is not rebuilt when the daemon restarts, so a handoff that trusts
/// the map alone kills nothing, finds nothing to clean up, and re-queues the
/// stage anyway — putting a second agent into the worktree the first is still
/// writing. While the original agent is alive the stage must stay
/// `NeedsHandoff`.
#[test]
fn handoff_does_not_requeue_while_an_untracked_agent_is_still_alive() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let session = recorded_session(&work);
    write_pid_file(&work, &session, None);

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    // The daemon restarted: nothing in `active_sessions` for this stage.
    assert!(orchestrator.active_sessions.is_empty());

    orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .unwrap();

    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::NeedsHandoff,
        "a stage whose agent survived the kill must not go back to Queued"
    );
    assert!(!graph_has_ready_stage(&orchestrator.graph, "test-stage"));
}

/// The other half of the same guard: once nothing is running for the stage, the
/// handoff must re-queue it as before. Here the recorded session's PID identity
/// cannot match this process, so the probe reports it dead.
#[test]
fn handoff_requeues_once_the_stage_has_no_live_agent() {
    let temp = handoff_work_dir();
    let work = temp.path().join(".work");
    executing_stage(&work);

    let session = recorded_session(&work);
    write_pid_file(&work, &session, Some(u64::MAX));

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();

    orchestrator
        .on_needs_handoff(&session.id, "test-stage")
        .unwrap();

    assert_eq!(
        load_stage("test-stage", &work).unwrap().status,
        StageStatus::Queued
    );
    assert!(graph_has_ready_stage(&orchestrator.graph, "test-stage"));
}

#[test]
fn test_needs_handoff_transitions_stage_to_queued() {
    // Verify that the NeedsHandoff -> Queued transition works correctly
    // This is the core logic that on_needs_handoff relies on
    let mut stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };

    // Transition: Executing -> NeedsHandoff
    stage.try_mark_needs_handoff().unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHandoff);

    // Transition: NeedsHandoff -> Queued (the fix)
    stage.try_mark_queued().unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn test_needs_handoff_requeues_in_graph() {
    // Verify that graph correctly tracks the stage as ready after re-queuing
    let mut graph = create_test_graph();

    // Initially the stage should be ready (WaitingForDeps with no deps = ready)
    assert!(graph_has_ready_stage(&graph, "test-stage"));

    // Mark as executing
    graph.mark_executing("test-stage").unwrap();
    assert!(!graph_has_ready_stage(&graph, "test-stage"));

    // Mark as NeedsHandoff then re-queue
    graph
        .mark_status("test-stage", StageStatus::NeedsHandoff)
        .unwrap();
    graph.mark_queued("test-stage").unwrap();

    // Stage should be ready again for the next poll cycle
    assert!(graph_has_ready_stage(&graph, "test-stage"));
}

#[test]
fn test_budget_exceeded_transitions_to_queued() {
    // Verify the full budget exceeded transition path:
    // Executing -> NeedsHandoff -> Queued
    let mut stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };

    // Simulate budget exceeded flow
    stage.accumulate_attempt_time(chrono::Utc::now());
    stage.try_mark_needs_handoff().unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHandoff);

    // Re-queue for continuation
    stage.try_mark_queued().unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn handoff_requeue_preserves_concurrent_unrelated_field() {
    use std::sync::{Arc, Barrier};

    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().to_path_buf();
    let stage = Stage {
        id: "event-race".to_string(),
        name: "Event race".to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    create_stage(&stage, &work_dir).unwrap();

    let handoff_marked = Arc::new(Barrier::new(2));
    let concurrent_done = Arc::new(Barrier::new(2));
    let event_dir = work_dir.clone();
    let event_marked = Arc::clone(&handoff_marked);
    let event_done = Arc::clone(&concurrent_done);
    let event = std::thread::spawn(move || {
        update_stage("event-race", &event_dir, |stage| {
            mark_needs_handoff(stage, Utc::now())
        })
        .unwrap();
        event_marked.wait();
        event_done.wait();
        update_stage("event-race", &event_dir, requeue_after_handoff).unwrap();
    });

    handoff_marked.wait();
    update_stage("event-race", &work_dir, |stage| {
        stage.dispute_count = 9;
        Ok(())
    })
    .unwrap();
    concurrent_done.wait();
    event.join().unwrap();

    let stage = load_stage("event-race", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.dispute_count, 9);
}
