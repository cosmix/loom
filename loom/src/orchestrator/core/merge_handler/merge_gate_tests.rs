//! Git-backed acceptance tests for the merge gate (owner decision 9), plus
//! pure unit tests for its path logic.

use super::*;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;
use crate::test_git::{git, git_unchecked, TestRepo};
use serial_test::serial;
use std::path::{Path, PathBuf};

fn orchestrator_for(root: &Path, work_dir: &Path) -> Orchestrator {
    let config = OrchestratorConfig {
        work_dir: work_dir.to_path_buf(),
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        kill_confirm_timeout: std::time::Duration::from_millis(300),
        ..Default::default()
    };
    let constructed = Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap());
    constructed.unwrap()
}

fn save_completed_unmerged_stage(stage_id: &str, work_dir: &Path) {
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Completed;
    stage.merged = false;
    let root = crate::fs::work_dir::WorkDir::new(work_dir)
        .unwrap()
        .main_project_root()
        .unwrap();
    stage.completed_commit =
        Some(crate::git::get_branch_head(&format!("loom/{stage_id}"), &root).unwrap());
    crate::verify::transitions::save_stage(&stage, work_dir).unwrap();
}

/// A repo with `main` at a seed commit and a `loom/<stage_id>` branch, built
/// in its own worktree, one commit ahead writing every `(path, content)` in
/// `touch`. Returns the tempdir (kept alive for the test's duration) and the
/// worktree path.
fn repo_with_stage_branch(stage_id: &str, touch: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let temp = TestRepo::builder()
        .identity("t", "t@t.com")
        .seed("seed.txt", "seed", "seed")
        .create()
        .into_temp_dir();
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    crate::git::worktree::capture_main_identity(root, &work_dir).unwrap();

    let worktree_path = root.join(".worktrees").join(stage_id);
    let branch = format!("loom/{stage_id}");
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            worktree_path.to_str().unwrap(),
        ],
    );
    for (path, content) in touch {
        let file_path = worktree_path.join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file_path, content).unwrap();
        git(&worktree_path, &["add", path]);
    }
    git(&worktree_path, &["commit", "-m", "stage work"]);

    (temp, worktree_path)
}

/// [`repo_with_stage_branch`], plus a `main`-side commit that conflicts with
/// the stage branch's `shared.txt` (an add/add conflict), so a real merge
/// attempt would need conflict resolution.
fn repo_with_conflicting_stage_branch(stage_id: &str) -> (tempfile::TempDir, PathBuf) {
    let (temp, worktree_path) = repo_with_stage_branch(
        stage_id,
        &[
            ("shared.txt", "stage change\n"),
            (".claude/settings.json", "{}\n"),
        ],
    );
    let root = temp.path();
    std::fs::write(root.join("shared.txt"), "main change\n").unwrap();
    git(root, &["add", "shared.txt"]);
    git(root, &["commit", "-m", "main diverges"]);
    (temp, worktree_path)
}

fn assert_held(root: &Path, work_dir: &Path, stage_id: &str, offending_path: &str) {
    let mut orchestrator = orchestrator_for(root, work_dir);
    assert!(
        !orchestrator.try_auto_merge(stage_id),
        "a control-path-touching branch must not merge"
    );
    let reloaded = crate::verify::transitions::load_stage(stage_id, work_dir).unwrap();
    assert_eq!(reloaded.status, StageStatus::NeedsHumanReview);
    assert!(!reloaded.merged);
    let reason = reloaded
        .review_reason
        .expect("routing to human review must record review_reason");
    assert!(
        reason.contains(offending_path),
        "review_reason must name the offending path {offending_path}: {reason}"
    );
    assert!(
        orchestrator.active_sessions.is_empty(),
        "a held stage must never get a merge-resolution session"
    );
}

#[test]
#[serial]
fn a_branch_touching_claude_settings_is_held() {
    let stage_id = "touches-claude";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[(".claude/settings.json", "{}\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".claude/settings.json");
}

#[test]
#[serial]
fn a_branch_touching_mcp_json_is_held() {
    let stage_id = "touches-mcp";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[(".mcp.json", "{}\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".mcp.json");
}

#[test]
#[serial]
fn a_branch_touching_dot_loom_is_held() {
    let stage_id = "touches-dot-loom";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[(".loom/x", "x\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".loom/x");
}

#[test]
#[serial]
fn a_branch_touching_the_configured_hooks_dir_is_held() {
    let stage_id = "touches-hooks";
    let (temp, _worktree) =
        repo_with_stage_branch(stage_id, &[("loom/.githooks/pre-commit", "#!/bin/sh\n")]);
    let root = temp.path();
    git(root, &["config", "core.hooksPath", "loom/.githooks"]);
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, "loom/.githooks/pre-commit");
}

#[test]
#[serial]
fn a_branch_touching_only_ordinary_source_merges_as_before() {
    let stage_id = "ordinary-source";
    let (temp, _worktree) = repo_with_stage_branch(stage_id, &[("src/x.rs", "fn x() {}\n")]);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    let mut orchestrator = orchestrator_for(root, &work_dir);
    assert!(
        orchestrator.try_auto_merge(stage_id),
        "a branch touching no control path must merge exactly as before"
    );
    let reloaded = crate::verify::transitions::load_stage(stage_id, &work_dir).unwrap();
    assert!(reloaded.merged);
    assert!(git_unchecked(root, &["show", "main:src/x.rs"])
        .status
        .success());
}

#[test]
#[serial]
fn a_conflicting_branch_touching_claude_settings_spawns_no_resolver() {
    let stage_id = "conflicting-and-touches-claude";
    let (temp, _worktree) = repo_with_conflicting_stage_branch(stage_id);
    let root = temp.path();
    let work_dir = root.join(".loom").join("work");
    save_completed_unmerged_stage(stage_id, &work_dir);

    assert_held(root, &work_dir, stage_id, ".claude/settings.json");

    let mut orchestrator = orchestrator_for(root, &work_dir);
    assert_eq!(
        orchestrator.spawn_merge_resolution_sessions().unwrap(),
        0,
        "a held stage is no longer MergeConflict/MergeBlocked, so the resolver \
         spawn loop must not touch it"
    );
}
