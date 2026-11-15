//! Contract tests for `Orchestrator::block_stage`: one locked transition that persists
//! `Blocked` and then marks the graph, never the graph after a refused persist.
use super::stage_block::StageBlock;
use super::{Orchestrator, OrchestratorConfig};
use crate::models::failure::FailureType;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::verify::transitions::{load_stage, save_stage};
use tempfile::TempDir;

fn fixture(stage: &Stage) -> (TempDir, Orchestrator) {
    let temp = TempDir::new().unwrap();
    crate::fs::work_dir::write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    save_stage(stage, temp.path()).unwrap();
    let graph = ExecutionGraph::build(vec![StageDefinition {
        id: stage.id.clone(),
        name: stage.id.clone(),
        dependencies: stage.dependencies.clone(),
        ..Default::default()
    }])
    .unwrap();
    let orchestrator = Orchestrator::new(
        OrchestratorConfig {
            work_dir: temp.path().to_owned(),
            repo_root: temp.path().to_owned(),
            max_parallel_sessions: 1,
            enable_skill_routing: false,
            ..Default::default()
        },
        graph,
    )
    .unwrap();
    (temp, orchestrator)
}

fn stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.into(),
        name: id.into(),
        status,
        ..Default::default()
    }
}

fn graph_status(orchestrator: &Orchestrator, id: &str) -> StageStatus {
    orchestrator.graph.get_node(id).unwrap().status.clone()
}

#[test]
fn block_syncs_graph() {
    let (temp, mut orchestrator) = fixture(&stage("s1", StageStatus::Queued));
    let block = StageBlock::new(FailureType::InfrastructureError, vec!["boom".into()]);

    assert!(orchestrator.block_stage("s1", block));

    let persisted = load_stage("s1", temp.path()).unwrap();
    assert_eq!(persisted.status, StageStatus::Blocked);
    let info = persisted.failure_info.expect("failure_info persisted");
    assert_eq!(info.failure_type, FailureType::InfrastructureError);
    assert_eq!(info.evidence, vec!["boom".to_string()]);
    assert_eq!(graph_status(&orchestrator, "s1"), StageStatus::Blocked);
}

#[test]
fn refused_block_leaves_graph() {
    let (temp, mut orchestrator) = fixture(&stage("s1", StageStatus::Completed));
    let before = graph_status(&orchestrator, "s1");
    let block = StageBlock::new(FailureType::InfrastructureError, vec![]);

    assert!(!orchestrator.block_stage("s1", block));

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Completed
    );
    let after = graph_status(&orchestrator, "s1");
    assert_eq!(after, before);
    assert_ne!(after, StageStatus::Blocked);
}

#[test]
fn block_releases_session_when_asked() {
    let mut executing = stage("s1", StageStatus::Executing);
    executing.session = Some("sess-1".into());
    let (temp, mut orchestrator) = fixture(&executing);
    let block = StageBlock {
        release_session: true,
        close_reason: Some("why".into()),
        ..StageBlock::new(FailureType::InfrastructureError, vec!["boom".into()])
    };

    assert!(orchestrator.block_stage("s1", block));

    let persisted = load_stage("s1", temp.path()).unwrap();
    assert_eq!(persisted.status, StageStatus::Blocked);
    assert_eq!(persisted.session, None);
    assert_eq!(persisted.close_reason.as_deref(), Some("why"));
    assert_eq!(graph_status(&orchestrator, "s1"), StageStatus::Blocked);
}
