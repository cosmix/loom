//! A daemon pass that auto-merges a Completed stage must leave its graph node
//! merged, so dependents are ready in the graph as well as in the stage files.

use super::merge_lifecycle_e2e_tests_support::*;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::recovery::Recovery;
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::save_stage;

const DEPENDENT: &str = "t";

fn definition(id: &str, dependencies: Vec<String>) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: ".".to_string(),
        dependencies,
        ..Default::default()
    }
}

#[test]
fn sync_projects_the_merge_it_performs_so_dependents_are_ready() {
    let repo = repo_with_worktree();
    let root = repo.path();
    commit_file(&worktree(root), "stage.txt", "work\n");
    let graph = ExecutionGraph::build(vec![
        definition(ID, Vec::new()),
        definition(DEPENDENT, vec![ID.to_string()]),
    ])
    .unwrap();
    let mut orchestrator = orchestrator_with_graph(root, graph);
    let work_dir = orchestrator.config.work_dir.clone();
    let completed = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        completed_commit: Some(branch_tip(root)),
        ..Stage::default()
    };
    save_stage(&completed, &work_dir).unwrap();
    let waiting = Stage {
        id: DEPENDENT.to_string(),
        status: StageStatus::WaitingForDeps,
        dependencies: vec![ID.to_string()],
        ..Stage::default()
    };
    save_stage(&waiting, &work_dir).unwrap();

    orchestrator.sync_graph_with_stage_files().unwrap();

    assert!(on_disk(&orchestrator).merged, "auto-merge did not land");
    assert!(orchestrator.graph.get_node(ID).unwrap().merged);
    assert_eq!(
        orchestrator.graph.get_node(DEPENDENT).unwrap().status,
        StageStatus::Queued
    );
}
