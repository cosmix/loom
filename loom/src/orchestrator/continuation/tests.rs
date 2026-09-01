//! Tests for continuation module.

use super::*;
use crate::git::branch::branch_name_for_stage;
use crate::models::stage::StageStatus;
use crate::models::worktree::Worktree;
use std::fs;
use tempfile::TempDir;

fn create_test_work_dir() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join(".loom").join("work");

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

/// Plant a worktree fixture where a real one lives: `<repo>/.worktrees/<id>`,
/// with `repo` the temp directory `create_test_work_dir` rooted the state
/// directory under — NOT `work_dir.parent()`.
///
/// The single hop these tests used to take landed on `<repo>/.loom`, which is
/// exactly where `load_worktree_path` used to look, so production and the
/// fixtures agreed on a location no real worktree can ever occupy and the
/// suite stayed green over the bug. Pinning the real location is what makes
/// every test below fail if that hop is reintroduced.
fn create_test_worktree(stage_id: &str, project_root: &std::path::Path) -> Worktree {
    let worktree_path = Worktree::worktree_path(project_root, stage_id);
    fs::create_dir_all(&worktree_path).unwrap();

    let mut worktree = Worktree::new(
        stage_id.to_string(),
        worktree_path,
        branch_name_for_stage(stage_id),
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

fn write_v2_handoff(
    path: &std::path::Path,
    stage_id: &str,
    session_id: &str,
) -> std::path::PathBuf {
    let handoff = crate::handoff::HandoffV2::new(session_id, stage_id);
    fs::write(path, format!("---\n{}---\n", handoff.to_yaml().unwrap())).unwrap();
    path.to_path_buf()
}

#[test]
fn test_continuation_config_default() {
    let config = ContinuationConfig::default();
    assert!(!config.auto_spawn);
}

#[test]
fn direct_auto_spawn_is_refused_before_writing_a_session() {
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
    let stage = create_test_stage("unsafe-direct-spawn", &work_dir);
    let worktree = create_test_worktree(&stage.id, project_root);
    let error = continue_session(
        &stage,
        None,
        &worktree,
        &ContinuationConfig { auto_spawn: true },
        &work_dir,
    )
    .unwrap_err();

    assert!(format!("{error:#}").contains("Direct continuation spawning is unsafe"));
    assert_eq!(fs::read_dir(work_dir.join("sessions")).unwrap().count(), 0);
}

#[test]
fn test_prepare_continuation_with_handoff() {
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
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
    assert_eq!(context.branch, branch_name_for_stage(stage_id));
}

#[test]
fn test_prepare_continuation_without_handoff() {
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
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
fn prepare_continuation_selects_the_exact_outgoing_session() {
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
    let stage_id = "stage-exact";
    create_test_stage(stage_id, &work_dir);
    create_test_worktree(stage_id, project_root);
    crate::verify::transitions::update_stage(stage_id, &work_dir, |stage| {
        stage.session = Some("session-old".to_string());
        Ok(())
    })
    .unwrap();

    let exact = write_v2_handoff(
        &work_dir
            .join("handoffs")
            .join(format!("{stage_id}-handoff-001.md")),
        stage_id,
        "session-old",
    );
    write_v2_handoff(
        &work_dir
            .join("handoffs")
            .join(format!("{stage_id}-handoff-002.md")),
        stage_id,
        "session-other",
    );

    let context = prepare_continuation(stage_id, &work_dir).unwrap();
    assert_eq!(context.handoff_path.as_deref(), Some(exact.as_path()));
}

#[test]
fn prepare_continuation_surfaces_unreadable_handoff_uncertainty() {
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
    let stage_id = "stage-unreadable";
    create_test_stage(stage_id, &work_dir);
    create_test_worktree(stage_id, project_root);
    crate::verify::transitions::update_stage(stage_id, &work_dir, |stage| {
        stage.session = Some("session-old".to_string());
        Ok(())
    })
    .unwrap();
    fs::create_dir(
        work_dir
            .join("handoffs")
            .join(format!("{stage_id}-handoff-001.md")),
    )
    .unwrap();

    let error = prepare_continuation(stage_id, &work_dir).unwrap_err();
    assert!(format!("{error:#}").contains("Failed to read handoff file"));
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
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
    let stage_id = "stage-test-4";

    let stage = create_test_stage(stage_id, &work_dir);
    let worktree = create_test_worktree(stage_id, project_root);
    let handoff_path = create_test_handoff(stage_id, &work_dir);

    let config = ContinuationConfig { auto_spawn: false };

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
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
    let stage_id = "stage-test-5";

    let stage = create_test_stage(stage_id, &work_dir);
    let worktree = create_test_worktree(stage_id, project_root);

    let config = ContinuationConfig { auto_spawn: false };

    let session = continue_session(&stage, None, &worktree, &config, &work_dir)
        .expect("Should create continuation session without handoff");

    assert!(session.stage_id.is_some());
    assert_eq!(session.stage_id.unwrap(), stage_id);
}

#[test]
fn test_continue_session_invalid_status() {
    let (temp, work_dir) = create_test_work_dir();
    let project_root = temp.path();
    let stage_id = "stage-test-6";

    let mut stage = create_test_stage(stage_id, &work_dir);
    stage.status = StageStatus::Completed;

    let worktree = create_test_worktree(stage_id, project_root);

    let config = ContinuationConfig { auto_spawn: false };

    let result = continue_session(&stage, None, &worktree, &config, &work_dir);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("cannot be continued"));
}
