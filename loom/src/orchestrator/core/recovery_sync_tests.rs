//! Regression test for `sync_graph_with_stage_files` leaving an
//! already-`Executing` graph node alone.
//!
//! Split out of `recovery.rs`'s `mod tests` to keep that file under the
//! maintainability limit — the same trick `recovery_adoption_tests.rs` uses.

use super::*;
use tempfile::TempDir;

use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::schema::{Implementers, StageDefinition, StageSandboxConfig};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::save_stage;

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
