//! Tests for loom init command.

use super::cleanup::{cleanup_work_directory, prune_stale_worktrees};
use super::plan_setup::{create_stage_from_definition, initialize_with_plan};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::schema::{LoomConfig, LoomMetadata, StageDefinition};
use crate::verify::serialize_stage_to_markdown;
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a minimal valid plan file
fn create_test_plan(dir: &Path, stages: Vec<StageDefinition>) -> PathBuf {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            stages,
        },
    };

    let yaml = serde_yaml::to_string(&metadata).unwrap();
    let plan_content = format!(
        "# Test Plan\n\n## Overview\n\nTest plan for unit tests\n\n<!-- loom METADATA -->\n```yaml\n{yaml}```\n<!-- END loom METADATA -->\n"
    );

    let plan_path = dir.join("test-plan.md");
    fs::write(&plan_path, plan_content).unwrap();
    plan_path
}

#[test]
fn test_create_stage_from_definition_no_dependencies() {
    let stage_def = StageDefinition {
        id: "stage-1".to_string(),
        name: "Stage 1".to_string(),
        description: Some("Test stage".to_string()),
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec!["cargo test".to_string()],
        setup: vec![],
        files: vec!["src/*.rs".to_string()],
        auto_merge: None,
    };

    let stage = create_stage_from_definition(&stage_def, "plan-001");

    assert_eq!(stage.id, "stage-1");
    assert_eq!(stage.name, "Stage 1");
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.plan_id, Some("plan-001".to_string()));
    assert_eq!(stage.dependencies.len(), 0);
    assert_eq!(stage.acceptance.len(), 1);
}

#[test]
fn test_create_stage_from_definition_with_dependencies() {
    let stage_def = StageDefinition {
        id: "stage-2".to_string(),
        name: "Stage 2".to_string(),
        description: None,
        dependencies: vec!["stage-1".to_string()],
        parallel_group: Some("core".to_string()),
        acceptance: vec![],
        setup: vec!["cargo build".to_string()],
        files: vec![],
        auto_merge: None,
    };

    let stage = create_stage_from_definition(&stage_def, "plan-002");

    assert_eq!(stage.id, "stage-2");
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert_eq!(stage.dependencies, vec!["stage-1".to_string()]);
    assert_eq!(stage.parallel_group, Some("core".to_string()));
}

#[test]
fn test_serialize_stage_to_markdown_minimal() {
    let stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        description: None,
        status: StageStatus::Queued,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        plan_id: None,
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
        close_reason: None,
        auto_merge: None,
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
    };

    let content = serialize_stage_to_markdown(&stage).unwrap();

    assert!(content.starts_with("---\n"));
    assert!(content.contains("# Stage: Test Stage"));
    assert!(content.contains("**Status**: Queued"));
}

#[test]
fn test_serialize_stage_to_markdown_with_all_fields() {
    let stage = Stage {
        id: "full-stage".to_string(),
        name: "Full Stage".to_string(),
        description: Some("Detailed description".to_string()),
        status: StageStatus::Executing,
        dependencies: vec!["dep1".to_string(), "dep2".to_string()],
        parallel_group: Some("group1".to_string()),
        acceptance: vec!["test1".to_string(), "test2".to_string()],
        setup: vec![],
        files: vec!["file1.rs".to_string(), "file2.rs".to_string()],
        plan_id: Some("plan-123".to_string()),
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
        close_reason: None,
        auto_merge: None,
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
    };

    let content = serialize_stage_to_markdown(&stage).unwrap();

    assert!(content.contains("## Dependencies"));
    assert!(content.contains("- dep1"));
    assert!(content.contains("- dep2"));
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("- [ ] test1"));
    assert!(content.contains("- [ ] test2"));
    assert!(content.contains("## Files"));
    assert!(content.contains("- `file1.rs`"));
    assert!(content.contains("- `file2.rs`"));
}

#[test]
fn test_initialize_with_plan_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let nonexistent_path = temp_dir.path().join("nonexistent.md");

    let result = initialize_with_plan(&work_dir, &nonexistent_path);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}

#[test]
fn test_initialize_with_plan_creates_config() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let stage_def = StageDefinition {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
    };

    let plan_path = create_test_plan(temp_dir.path(), vec![stage_def]);

    let result = initialize_with_plan(&work_dir, &plan_path);

    assert!(result.is_ok());

    let config_path = work_dir.root().join("config.toml");
    assert!(config_path.exists());

    let config_content = fs::read_to_string(config_path).unwrap();
    assert!(config_content.contains("source_path"));
    assert!(config_content.contains("plan_id"));
}

#[test]
fn test_initialize_with_plan_creates_stage_files() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let stages = vec![
        StageDefinition {
            id: "stage-1".to_string(),
            name: "Stage One".to_string(),
            description: Some("First stage".to_string()),
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec!["cargo test".to_string()],
            setup: vec![],
            files: vec![],
            auto_merge: None,
        },
        StageDefinition {
            id: "stage-2".to_string(),
            name: "Stage Two".to_string(),
            description: None,
            dependencies: vec!["stage-1".to_string()],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
        },
    ];

    let plan_path = create_test_plan(temp_dir.path(), stages);

    let result = initialize_with_plan(&work_dir, &plan_path);

    assert!(result.is_ok());

    let stages_dir = work_dir.root().join("stages");
    assert!(stages_dir.exists());

    let stage_files: Vec<_> = fs::read_dir(stages_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();

    assert_eq!(stage_files.len(), 2);
}

#[test]
fn test_cleanup_work_directory_removes_existing() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");

    fs::create_dir_all(&work_dir).unwrap();
    fs::write(work_dir.join("test.txt"), "content").unwrap();

    assert!(work_dir.exists());

    let result = cleanup_work_directory(temp_dir.path());

    assert!(result.is_ok());
    assert!(!work_dir.exists());
}

#[test]
fn test_cleanup_work_directory_nonexistent_ok() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");

    assert!(!work_dir.exists());

    let result = cleanup_work_directory(temp_dir.path());

    assert!(result.is_ok());
}

#[test]
fn test_initialize_with_plan_invalid_yaml() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let invalid_plan = temp_dir.path().join("invalid.md");
    fs::write(
        &invalid_plan,
        "# Invalid Plan\n\n<!-- loom METADATA -->\n```yaml\ninvalid: yaml: content:\n```\n",
    )
    .unwrap();

    let result = initialize_with_plan(&work_dir, &invalid_plan);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("parse"));
}

#[test]
fn test_prune_stale_worktrees_does_not_fail() {
    let temp_dir = TempDir::new().unwrap();

    let result = prune_stale_worktrees(temp_dir.path());

    assert!(result.is_ok());
}

#[test]
fn test_cleanup_orphaned_tmux_sessions_does_not_fail() {
    use super::cleanup::cleanup_orphaned_tmux_sessions;

    let result = cleanup_orphaned_tmux_sessions();

    assert!(result.is_ok());
}
