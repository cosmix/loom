use super::Orchestrator;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;

#[test]
fn merge_probe_failure_does_not_consume_resolver_attempt_budget() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".work");
    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut stage = Stage::new("probe-failure".to_string(), None);
    stage.id = "probe-failure".to_string();
    stage.status = StageStatus::MergeConflict;
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();
    let mut orchestrator =
        Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(orchestrator.merge_resolver_attempts(&stage.id), 0);
    assert!(!orchestrator
        .merge_resolver_attempts_dir()
        .join(format!("{}.count", stage.id))
        .exists());
}
