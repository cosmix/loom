//! Tests for continuation module.

use super::*;
use crate::models::stage::StageStatus;
use crate::models::worktree::Worktree;
use crate::orchestrator::terminal::BackendType;
use std::fs;
use tempfile::TempDir;

fn create_test_work_dir() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join(".work");

    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::create_dir_all(work_dir.join("handoffs")).unwrap();
    fs::create_dir_all(work_dir.join("sessions")).unwrap();
    fs::create_dir_all(work_dir.join("signals")).unwrap();

    (temp_dir, work_dir)
}

fn create_test_stage(stage_id: &str, work_dir: &std::path::Path) -> crate::models::stage::Stage {
    let mut stage = crate::models::stage::Stage::new(
        "Test Stage".to_string(),
        Some("Test description".to_string()),
    );
    stage.id = stage_id.to_string();
    stage.status = StageStatus::NeedsHandoff;
    stage.worktree = Some(stage_id.to_string());

    let stage_path = work_dir.join("stages").join(format!("{stage_id}.md"));
    let yaml = serde_yaml::to_string(&stage).unwrap();
    let content = format!("---\n{yaml}---\n\n# Stage: {stage_id}\n");
    fs::write(stage_path, content).unwrap();

    stage
}

fn create_test_worktree(stage_id: &str, project_root: &std::path::Path) -> Worktree {
    let worktree_path = Worktree::worktree_path(project_root, stage_id);
    fs::create_dir_all(&worktree_path).unwrap();

    let mut worktree = Worktree::new(
        stage_id.to_string(),
        worktree_path,
        Worktree::branch_name(stage_id),
    );
    worktree.mark_active();
    worktree
}

fn create_test_handoff(stage_id: &str, work_dir: &std::path::Path) -> std::path::PathBuf {
    let handoff_content = format!(
        r#"# Handoff: Test Handoff

## Metadata

- **Date**: 2026-01-06
- **From**: runner-1 (developer)
- **To**: runner-2 (developer)
- **Track**: {stage_id}
- **Stage**: {stage_id}
- **Context**: 75%

## Goals

Test the continuation feature.

## Completed Work

- Created test stage

## Next Steps

1. Continue work on the stage
2. Verify continuation works
"#
    );

    let handoff_path = work_dir
        .join("handoffs")
        .join(format!("{stage_id}-handoff-001.md"));
    fs::write(&handoff_path, handoff_content).unwrap();
    handoff_path
}

#[test]
fn test_continuation_config_default() {
    let config = ContinuationConfig::default();
    assert_eq!(config.backend_type, BackendType::Native);
    assert!(config.auto_spawn);
}

#[test]
fn test_prepare_continuation_with_handoff() {
    let (_temp, work_dir) = create_test_work_dir();
    let project_root = work_dir.parent().unwrap();
    let stage_id = "stage-test-1";

    create_test_stage(stage_id, &work_dir);
    create_test_worktree(stage_id, project_root);
    let handoff_path = create_test_handoff(stage_id, &work_dir);

    let context =
        prepare_continuation(stage_id, &work_dir).expect("Should prepare continuation context");

    assert_eq!(context.stage.id, stage_id);
    assert!(context.handoff_path.is_some());
    assert_eq!(
        context.handoff_path.unwrap().canonicalize().unwrap(),
        handoff_path.canonicalize().unwrap()
    );
    assert!(context.worktree_path.exists());
    assert_eq!(context.branch, format!("loom/{stage_id}"));
}

#[test]
fn test_prepare_continuation_without_handoff() {
    let (_temp, work_dir) = create_test_work_dir();
    let project_root = work_dir.parent().unwrap();
    let stage_id = "stage-test-2";

    create_test_stage(stage_id, &work_dir);
    create_test_worktree(stage_id, project_root);

    let context =
        prepare_continuation(stage_id, &work_dir).expect("Should prepare continuation context");

    assert_eq!(context.stage.id, stage_id);
    assert!(context.handoff_path.is_none());
    assert!(context.worktree_path.exists());
}

#[test]
fn test_prepare_continuation_stage_not_found() {
    let (_temp, work_dir) = create_test_work_dir();

    let result = prepare_continuation("nonexistent-stage", &work_dir);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Stage file not found"));
}

#[test]
fn test_load_handoff_content() {
    let (_temp, work_dir) = create_test_work_dir();
    let stage_id = "stage-test-3";
    let handoff_path = create_test_handoff(stage_id, &work_dir);

    let content = load_handoff_content(&handoff_path).expect("Should load handoff content");

    assert!(content.contains("# Handoff: Test Handoff"));
    assert!(content.contains(&format!("**Track**: {stage_id}")));
    assert!(content.contains("## Next Steps"));
}

#[test]
fn test_load_handoff_content_not_found() {
    let (_temp, work_dir) = create_test_work_dir();
    let fake_path = work_dir.join("handoffs").join("nonexistent.md");

    let result = load_handoff_content(&fake_path);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Handoff file does not exist"));
}

#[test]
fn test_continue_session_with_handoff() {
    let (_temp, work_dir) = create_test_work_dir();
    let project_root = work_dir.parent().unwrap();
    let stage_id = "stage-test-4";

    let stage = create_test_stage(stage_id, &work_dir);
    let worktree = create_test_worktree(stage_id, project_root);
    let handoff_path = create_test_handoff(stage_id, &work_dir);

    let config = ContinuationConfig {
        backend_type: BackendType::Native,
        auto_spawn: false,
    };

    let session = continue_session(&stage, Some(&handoff_path), &worktree, &config, &work_dir)
        .expect("Should create continuation session");

    assert!(session.stage_id.is_some());
    assert_eq!(session.stage_id.unwrap(), stage_id);
    assert!(session.worktree_path.is_some());

    let signal_path = work_dir.join("signals").join(format!("{}.md", session.id));
    assert!(signal_path.exists());

    let signal_content = fs::read_to_string(signal_path).unwrap();
    assert!(signal_content.contains(&format!("# Signal: {}", session.id)));
    assert!(signal_content.contains(&format!("**Stage**: {stage_id}")));
}

#[test]
fn test_continue_session_without_handoff() {
    let (_temp, work_dir) = create_test_work_dir();
    let project_root = work_dir.parent().unwrap();
    let stage_id = "stage-test-5";

    let stage = create_test_stage(stage_id, &work_dir);
    let worktree = create_test_worktree(stage_id, project_root);

    let config = ContinuationConfig {
        backend_type: BackendType::Native,
        auto_spawn: false,
    };

    let session = continue_session(&stage, None, &worktree, &config, &work_dir)
        .expect("Should create continuation session without handoff");

    assert!(session.stage_id.is_some());
    assert_eq!(session.stage_id.unwrap(), stage_id);
}

#[test]
fn test_continue_session_invalid_status() {
    let (_temp, work_dir) = create_test_work_dir();
    let project_root = work_dir.parent().unwrap();
    let stage_id = "stage-test-6";

    let mut stage = create_test_stage(stage_id, &work_dir);
    stage.status = StageStatus::Completed;

    let worktree = create_test_worktree(stage_id, project_root);

    let config = ContinuationConfig {
        backend_type: BackendType::Native,
        auto_spawn: false,
    };

    let result = continue_session(&stage, None, &worktree, &config, &work_dir);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("cannot be continued"));
}
