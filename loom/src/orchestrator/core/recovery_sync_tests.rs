//! Regression test for `sync_graph_with_stage_files` leaving an
//! already-`Executing` graph node alone.
//!
//! Split out of `recovery.rs`'s `mod tests` to keep that file under the
//! maintainability limit — the same trick `recovery_adoption_tests.rs` uses.

use super::*;
use tempfile::TempDir;

use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::orchestrator::core::event_handler::EventHandler;
use crate::orchestrator::core::stage_executor::StageExecutor;
use crate::orchestrator::core::OrchestratorConfig;
use crate::orchestrator::monitor::MonitorEvent;
use crate::plan::schema::{Implementers, StageDefinition, StageSandboxConfig};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{save_stage, update_stage};

fn minimal_stage_definition(id: &str) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
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
        skills: vec![],
    }
}

/// Regression test for the per-tick "Failed to sync graph status" warning:
/// once a node is already `Executing` in the graph, `sync_graph_with_stage_files`
/// must not call `mark_executing` again (it only accepts `Queued -> Executing`
/// and would bail every 5-second tick for the life of the stage).
#[test]
fn sync_leaves_an_already_executing_node_executing_without_erroring() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().to_path_buf();
    write_terminal_config(
        &work_dir,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();

    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = StageStatus::Executing;
    save_stage(&stage, &work_dir).unwrap();

    let mut graph = ExecutionGraph::build(vec![minimal_stage_definition("alpha")]).unwrap();
    graph.mark_queued("alpha").unwrap();
    graph.mark_executing("alpha").unwrap();

    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut orchestrator = Orchestrator::new(config, graph).unwrap();

    orchestrator
        .sync_graph_with_stage_files()
        .expect("sync must not error when the graph node is already Executing");
    assert_eq!(
        orchestrator.graph.get_node("alpha").unwrap().status,
        StageStatus::Executing
    );
}

fn resume_orchestrator(temp: &TempDir, graph: ExecutionGraph) -> Orchestrator {
    let work_dir = temp.path().to_path_buf();
    write_terminal_config(
        &work_dir,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    Orchestrator::new(
        OrchestratorConfig {
            work_dir,
            repo_root: temp.path().to_path_buf(),
            enable_skill_routing: false,
            ..Default::default()
        },
        graph,
    )
    .unwrap()
}

fn stage_with_status(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        status,
        ..Stage::default()
    }
}

fn waiting_graph_for(id: &str) -> ExecutionGraph {
    let mut graph = ExecutionGraph::build(vec![minimal_stage_definition(id)]).unwrap();
    graph.mark_executing(id).unwrap();
    graph.mark_status(id, StageStatus::WaitingForInput).unwrap();
    graph
}

#[test]
fn restart_sync_resumes_waiting_graph_node() {
    let temp = TempDir::new().unwrap();
    save_stage(
        &stage_with_status("alpha", StageStatus::Executing),
        temp.path(),
    )
    .unwrap();
    let mut orchestrator = resume_orchestrator(&temp, waiting_graph_for("alpha"));

    orchestrator.sync_graph_with_stage_files().unwrap();

    assert_eq!(
        orchestrator.graph.get_node("alpha").unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn restart_sync_does_not_resume_unrelated_graph_mismatch() {
    let temp = TempDir::new().unwrap();
    save_stage(
        &stage_with_status("alpha", StageStatus::Executing),
        temp.path(),
    )
    .unwrap();
    let mut graph = ExecutionGraph::build(vec![minimal_stage_definition("alpha")]).unwrap();
    graph.force_status("alpha", StageStatus::Blocked).unwrap();
    let mut orchestrator = resume_orchestrator(&temp, graph);

    orchestrator.sync_graph_with_stage_files().unwrap();

    assert_eq!(
        orchestrator.graph.get_node("alpha").unwrap().status,
        StageStatus::Blocked
    );
}

#[test]
fn restart_sync_preserves_executing_dependent_before_dependency_sync() {
    let temp = TempDir::new().unwrap();
    save_stage(
        &stage_with_status("beta", StageStatus::Executing),
        temp.path(),
    )
    .unwrap();
    let mut alpha = stage_with_status("alpha", StageStatus::Completed);
    alpha.merged = true;
    alpha.stage_type = crate::models::stage::StageType::Knowledge;
    save_stage(&alpha, temp.path()).unwrap();

    let graph = ExecutionGraph::build(dependent_definitions()).unwrap();
    let mut orchestrator = resume_orchestrator(&temp, graph);

    orchestrator.sync_resumed_node("beta");
    assert_eq!(
        orchestrator.graph.get_node("beta").unwrap().status,
        StageStatus::Executing
    );
    orchestrator.sync_graph_with_stage_files().unwrap();

    assert_eq!(
        orchestrator.graph.get_node("alpha").unwrap().status,
        StageStatus::Completed
    );
    assert_eq!(
        orchestrator.graph.get_node("beta").unwrap().status,
        StageStatus::Executing
    );
    assert!(!orchestrator
        .graph
        .ready_stages()
        .iter()
        .any(|node| node.id == "beta"));
    assert_eq!(orchestrator.start_ready_stages().unwrap(), 0);
}

#[test]
fn live_resume_and_restart_keep_dependencies_coherent() {
    let temp = TempDir::new().unwrap();
    let definitions = dependent_definitions();
    let mut orchestrator = waiting_orchestrator(&temp, &definitions);

    send_resume_events(&mut orchestrator, 2);
    assert_eq!(
        orchestrator.graph.get_node("alpha").unwrap().status,
        StageStatus::Executing
    );

    let mut restarted = resume_orchestrator(&temp, ExecutionGraph::build(definitions).unwrap());
    restarted.sync_graph_with_stage_files().unwrap();
    assert_eq!(
        restarted.graph.get_node("alpha").unwrap().status,
        StageStatus::Executing
    );
    restarted.graph.mark_completed("alpha").unwrap();
    restarted.graph.mark_merged("alpha").unwrap();
    assert_eq!(
        restarted.graph.get_node("beta").unwrap().status,
        StageStatus::Queued
    );
}

fn dependent_definitions() -> Vec<StageDefinition> {
    let mut beta = minimal_stage_definition("beta");
    beta.dependencies = vec!["alpha".to_string()];
    vec![minimal_stage_definition("alpha"), beta]
}

fn waiting_orchestrator(temp: &TempDir, definitions: &[StageDefinition]) -> Orchestrator {
    let mut graph = ExecutionGraph::build(definitions.to_vec()).unwrap();
    graph.mark_executing("alpha").unwrap();
    graph
        .mark_status("alpha", StageStatus::WaitingForInput)
        .unwrap();
    save_stage(
        &stage_with_status("alpha", StageStatus::WaitingForInput),
        temp.path(),
    )
    .unwrap();
    let mut orchestrator = resume_orchestrator(temp, graph);
    EventHandler::handle_events(
        &mut orchestrator,
        vec![MonitorEvent::StageWaitingForInput {
            stage_id: "alpha".to_string(),
            session_id: None,
        }],
    )
    .unwrap();
    update_stage("alpha", temp.path(), |stage| {
        stage.try_transition(StageStatus::Executing)
    })
    .unwrap();
    orchestrator
}

fn send_resume_events(orchestrator: &mut Orchestrator, count: usize) {
    let events = (0..count)
        .map(|_| MonitorEvent::StageResumedExecution {
            stage_id: "alpha".to_string(),
        })
        .collect();
    EventHandler::handle_events(orchestrator, events).unwrap();
}

#[test]
fn stale_or_newer_resume_event_does_not_change_waiting_graph() {
    for disk_status in [StageStatus::WaitingForInput, StageStatus::Completed] {
        let temp = TempDir::new().unwrap();
        save_stage(&stage_with_status("alpha", disk_status), temp.path()).unwrap();
        let mut orchestrator = resume_orchestrator(&temp, waiting_graph_for("alpha"));

        EventHandler::handle_events(
            &mut orchestrator,
            vec![MonitorEvent::StageResumedExecution {
                stage_id: "alpha".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(
            orchestrator.graph.get_node("alpha").unwrap().status,
            StageStatus::WaitingForInput
        );
    }
}

fn restart_stage(id: &str, session_id: &str) -> Stage {
    Stage {
        id: id.to_string(),
        session: Some(session_id.to_string()),
        status: StageStatus::Executing,
        ..Stage::default()
    }
}

fn restart_session(stage_id: &str) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session
}

#[test]
fn restart_ignores_historical_dead_record_and_restores_current_live_session() {
    let mut historical = restart_session("stage-a");
    historical.id = "historical-dead".to_string();
    historical.status = crate::models::session::SessionStatus::Crashed;
    let mut current = restart_session("stage-a");
    current.id = "current-live".to_string();
    current.status = crate::models::session::SessionStatus::Running;
    let stage = restart_stage("stage-a", &current.id);
    let mut active = std::collections::HashMap::new();

    assert!(!register_live_current_session(
        &mut active,
        &stage,
        &historical
    ));
    assert!(register_live_current_session(&mut active, &stage, &current));
    assert_eq!(
        active.get("stage-a").map(|s| s.id.as_str()),
        Some("current-live")
    );
}

#[test]
fn restart_restores_multiple_surviving_sessions_into_capacity_accounting() {
    let mut active = std::collections::HashMap::new();
    for stage_id in ["stage-a", "stage-b", "stage-c"] {
        let session = restart_session(stage_id);
        let stage = restart_stage(stage_id, &session.id);
        assert!(register_live_current_session(&mut active, &stage, &session));
    }

    let max_parallel_sessions = 4usize;
    assert_eq!(active.len(), 3);
    assert_eq!(max_parallel_sessions.saturating_sub(active.len()), 1);
}
