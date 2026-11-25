//! `block_stage` edges the frozen contract tests leave open: the defaults of
//! `StageBlock::new`, a stage the graph does not hold, and a missing stage file.
use super::*;
use crate::models::stage::Stage;
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::verify::transitions::{load_stage, save_stage};
use tempfile::TempDir;

/// An orchestrator over `on_disk` stage files whose graph holds `in_graph`.
fn fixture(on_disk: &[Stage], in_graph: &[&str]) -> (TempDir, Orchestrator) {
    let temp = TempDir::new().unwrap();
    for stage in on_disk {
        save_stage(stage, temp.path()).unwrap();
    }
    let definitions = in_graph
        .iter()
        .map(|id| StageDefinition {
            id: (*id).into(),
            name: (*id).into(),
            ..Default::default()
        })
        .collect();
    let orchestrator = Orchestrator::new(
        OrchestratorConfig {
            work_dir: temp.path().to_owned(),
            repo_root: temp.path().to_owned(),
            max_parallel_sessions: 1,
            enable_skill_routing: false,
            ..Default::default()
        },
        ExecutionGraph::build(definitions).unwrap(),
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

#[test]
fn a_default_block_keeps_the_session_and_close_reason() {
    let mut executing = stage("s1", StageStatus::Executing);
    executing.session = Some("sess-1".into());
    executing.close_reason = Some("earlier".into());
    let (temp, mut orchestrator) = fixture(&[executing], &["s1"]);

    let block = StageBlock::new(FailureType::InfrastructureError, vec!["boom".into()]);
    assert!(orchestrator.block_stage("s1", block));

    let persisted = load_stage("s1", temp.path()).unwrap();
    assert_eq!(persisted.status, StageStatus::Blocked);
    assert_eq!(persisted.session.as_deref(), Some("sess-1"));
    assert_eq!(persisted.close_reason.as_deref(), Some("earlier"));
}

#[test]
fn a_stage_missing_from_the_graph_still_reports_the_persisted_block() {
    let (temp, mut orchestrator) = fixture(&[stage("s1", StageStatus::Queued)], &[]);

    let block = StageBlock::new(FailureType::InfrastructureError, Vec::new());
    assert!(orchestrator.block_stage("s1", block));

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Blocked
    );
    assert!(orchestrator.graph.get_node("s1").is_none());
}

#[test]
fn a_missing_stage_file_reports_no_block_and_leaves_the_graph() {
    let (_temp, mut orchestrator) = fixture(&[], &["s1"]);
    let before = orchestrator.graph.get_node("s1").unwrap().status.clone();

    let block = StageBlock::new(FailureType::InfrastructureError, Vec::new());
    assert!(!orchestrator.block_stage("s1", block));

    assert_eq!(orchestrator.graph.get_node("s1").unwrap().status, before);
}
