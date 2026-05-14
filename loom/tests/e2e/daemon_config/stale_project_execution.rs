//! Regression test: stale [project_execution] table in .work/config.toml
//!
//! After the collapse-backend-scaffolding refactor, `[project_execution]` is no
//! longer read or written by loom. A config.toml that still carries this legacy
//! section (e.g. from an old `loom init --backend native` run) must be ignored
//! silently — the orchestrator must start and behave identically to a config
//! without the section.

use loom::orchestrator::{Orchestrator, OrchestratorConfig};
use loom::plan::graph::ExecutionGraph;
use std::time::Duration;
use tempfile::TempDir;

use super::create_stage_def;

/// Write a minimal `.work/config.toml` that includes a stale `[project_execution]`
/// table along with a valid `[plan]` section.
fn write_stale_config(work_dir: &std::path::Path) {
    let config_content = r#"# loom Configuration
[plan]
source_path = "doc/plans/PLAN-test.md"
plan_id = "test-plan"
plan_name = "Test Plan"
base_branch = "main"

# Legacy section — must be silently ignored after the backend collapse.
[project_execution]
backend = "native"
"#;
    std::fs::write(work_dir.join("config.toml"), config_content).unwrap();
}

#[test]
fn test_orchestrator_config_ignores_stale_project_execution() {
    // Prove that building an OrchestratorConfig with a work_dir whose
    // config.toml contains a stale [project_execution] section does NOT cause
    // a panic, parse failure, or any behavioural difference from a clean config.

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    write_stale_config(&work_dir);

    // OrchestratorConfig construction must succeed — no error from the stale key.
    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp_dir.path().to_path_buf(),
        manual_mode: true,
        poll_interval: Duration::from_millis(50),
        ..Default::default()
    };

    // build_execution_graph reads config.toml; it must not fail on the stale section.
    // We call it indirectly by constructing an Orchestrator with an empty graph.
    let graph = ExecutionGraph::build(vec![create_stage_def("s1", "Stage 1", vec![])])
        .expect("graph should build");

    // Orchestrator::new should succeed regardless of the stale section.
    let orchestrator = Orchestrator::new(config, graph);
    assert!(
        orchestrator.is_ok(),
        "Orchestrator must start even when config.toml has a stale [project_execution] table"
    );
}

#[test]
fn test_base_branch_parsed_despite_stale_project_execution() {
    // Verify that the real config fields (e.g. base_branch) are still readable
    // when a stale [project_execution] section is present in config.toml.

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();

    write_stale_config(&work_dir);

    // loom's config parser must extract base_branch normally.
    let base_branch = loom::fs::parse_base_branch_from_config(&work_dir)
        .expect("parse_base_branch_from_config must succeed with stale section");

    assert_eq!(
        base_branch,
        Some("main".to_string()),
        "base_branch from [plan] must be read correctly despite stale [project_execution]"
    );
}
