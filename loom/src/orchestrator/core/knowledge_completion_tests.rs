//! A knowledge stage completed through the broker arrives Completed with
//! `merged = true` (`daemon::server::control_complete`). The daemon then has
//! nothing to merge, and the stage's dependents start through the ordinary
//! graph sync.

use crate::models::stage::{Stage, StageStatus, StageType};
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage};

use super::super::recovery::Recovery;
use super::super::{Orchestrator, OrchestratorConfig};

fn definition(
    id: &str,
    dependencies: &[&str],
    stage_type: Option<crate::plan::schema::StageType>,
) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        dependencies: dependencies.iter().map(|dep| dep.to_string()).collect(),
        working_dir: ".".to_string(),
        stage_type,
        ..Default::default()
    }
}

fn stage(id: &str, status: StageStatus, dependencies: &[&str]) -> Stage {
    let mut stage = Stage::new(id.to_string(), None);
    stage.id = id.to_string();
    stage.status = status;
    stage.dependencies = dependencies.iter().map(|dep| dep.to_string()).collect();
    stage
}

/// An orchestrator over the plan "notes (knowledge) -> build", on the tmux
/// lane so that building it never probes the host for a terminal emulator.
fn knowledge_then_build(work_dir: &std::path::Path, repo_root: std::path::PathBuf) -> Orchestrator {
    std::fs::write(
        work_dir.join("config.toml"),
        "[terminal]\nbackend = \"tmux\"\n",
    )
    .unwrap();
    let knowledge = Some(crate::plan::schema::StageType::Knowledge);
    let graph = ExecutionGraph::build(vec![
        definition("notes", &[], knowledge),
        definition("build", &["notes"], None),
    ])
    .unwrap();
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root,
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, graph).unwrap()
}

#[test]
fn a_broker_completed_knowledge_stage_is_not_merged_again_and_releases_its_dependents() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().to_path_buf();
    let work_dir = repo_root.join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    let mut notes = stage("notes", StageStatus::Completed, &[]);
    notes.stage_type = StageType::Knowledge;
    notes.merged = true;
    save_stage(&notes, &work_dir).unwrap();
    let build = stage("build", StageStatus::WaitingForDeps, &["notes"]);
    save_stage(&build, &work_dir).unwrap();
    let mut orchestrator = knowledge_then_build(&work_dir, repo_root);

    // The directory is not a git repository, so a merge attempt could only
    // fail and move the stage to MergeBlocked.
    assert!(orchestrator.try_auto_merge("notes"));
    let after = load_stage("notes", &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Completed);
    assert!(after.merged);

    orchestrator.sync_graph_with_stage_files().unwrap();
    let ready: Vec<String> = orchestrator
        .graph
        .ready_stages()
        .iter()
        .map(|node| node.id.clone())
        .collect();
    assert_eq!(ready, vec!["build".to_string()]);
}
