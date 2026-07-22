//! Regression tests for `attempt_progressive_merge` (PLAN-fix-phantom-merge.md)
//! and the deferred-cleanup gate `should_defer_cleanup`.
//!
//! Historically, the `NoBranch` arm of the inner match wrote `merged = true`
//! under the assumption that "branch already cleaned up" implied "already
//! merged." That assumption is wrong: if the branch is missing before any
//! merge attempt happened, we cannot verify anything landed. Fix 7 replaces
//! the arm with `MergeOutcome::Blocked` and does NOT write `merged = true`.
//!
//! Setting up a real merge that returns `NoBranch` naturally is tricky —
//! the function's precondition calls `get_branch_head` which errors if the
//! branch doesn't exist. The tests below build a minimal real-git repo
//! without the expected `loom/<stage-id>` branch, which is the same
//! observable condition (`NoBranch`) from the caller's perspective: the
//! function must return `Blocked` (or surface an error) and MUST NOT leave
//! the stage with `merged = true`.
//!
//! End-to-end phantom-merge prevention across recovery and daemon paths is
//! additionally exercised by the integration suite in `tests/phantom_merge.rs`.
use std::process::Command;

use tempfile::TempDir;

use super::super::progressive_complete::{
    attempt_progressive_merge, should_defer_cleanup, MergeOutcome,
};
use crate::models::stage::{Stage, StageStatus};

/// Build a real git repo with a `.work` directory and a `config.toml` that
/// points at `main` as the base branch. Returns the repo root TempDir.
fn init_repo_with_work_dir() -> TempDir {
    let temp_dir = TempDir::new().expect("tempdir");
    let repo_root = temp_dir.path();

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_root)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(repo_root)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(repo_root)
        .output()
        .expect("git config name");
    std::fs::write(repo_root.join("README.md"), "r").expect("write README");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_root)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(repo_root)
        .output()
        .expect("git commit");
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(repo_root)
        .output()
        .expect("rename to main");

    // Create a minimal .work directory with config.toml so `get_merge_point`
    // can resolve to "main".
    let work_dir = repo_root.join(".work");
    std::fs::create_dir_all(&work_dir).expect("mkdir .work");
    std::fs::write(work_dir.join("config.toml"), "base_branch = \"main\"\n")
        .expect("write config.toml");

    temp_dir
}

fn make_stage(id: &str) -> Stage {
    let mut stage = Stage::new(id.to_string(), Some(format!("test {id}")));
    stage.id = id.to_string();
    stage.status = StageStatus::Executing;
    stage
}

/// Fix 7: `attempt_progressive_merge` must NOT set `merged = true` when
/// the stage branch is missing. The old NoBranch arm silently wrote
/// `merged = true` — a phantom merge.
///
/// Without a `loom/<stage-id>` branch, `merge_completed_stage` returns
/// `NoBranch`, which the new code translates to `MergeOutcome::Blocked`.
/// Some git-layer paths may surface the missing branch as an error instead;
/// either way, the invariant we care about is the same: the stage's
/// `merged` flag must remain false.
#[test]
fn no_branch_does_not_mark_merged() {
    let repo = init_repo_with_work_dir();
    let repo_root = repo.path();
    let work_dir = repo_root.join(".work");

    let mut stage = make_stage("stage-no-branch");
    assert!(!stage.merged, "precondition: stage starts unmerged");

    // No loom/stage-no-branch branch exists. The progressive merge should
    // refuse to mark the stage merged regardless of how the missing branch
    // surfaces (Blocked outcome, or an Err from the deeper git call).
    let outcome = attempt_progressive_merge(&mut stage, repo_root, &work_dir);

    match outcome {
        Ok(MergeOutcome::Blocked) => {
            // Fix 7's intended behavior.
        }
        Ok(MergeOutcome::Success) => {
            panic!(
                "phantom merge: NoBranch should not produce Success. stage.merged = {}",
                stage.merged
            );
        }
        Ok(MergeOutcome::Conflict) => {
            panic!("unexpected Conflict from missing branch");
        }
        Err(_) => {
            // Some implementations may surface missing branch as an error
            // (e.g., if `get_branch_head` is called before the NoBranch
            // check in a future refactor). Either way the assertion below
            // is what matters.
        }
    }

    assert!(
        !stage.merged,
        "regression: missing stage branch must NOT set merged=true (phantom merge prevention)"
    );
}

/// Creates `<repo_root>/.worktrees/<stage_id>` on disk so canonicalization
/// in `should_defer_cleanup` succeeds, and returns its path.
fn make_worktree_dir(repo_root: &std::path::Path, stage_id: &str) -> std::path::PathBuf {
    let worktree = repo_root.join(".worktrees").join(stage_id);
    std::fs::create_dir_all(&worktree).expect("mkdir worktree");
    worktree
}

#[test]
fn should_defer_cleanup_when_cwd_deep_inside_worktree() {
    let repo = TempDir::new().expect("tempdir");
    let repo_root = repo.path();
    let worktree = make_worktree_dir(repo_root, "stage-1");
    let nested = worktree.join("src").join("lib");
    std::fs::create_dir_all(&nested).expect("mkdir nested");

    assert!(should_defer_cleanup(&nested, repo_root, "stage-1"));
}

#[test]
fn should_defer_cleanup_when_cwd_at_worktree_root() {
    let repo = TempDir::new().expect("tempdir");
    let repo_root = repo.path();
    let worktree = make_worktree_dir(repo_root, "stage-1");

    assert!(should_defer_cleanup(&worktree, repo_root, "stage-1"));
}

#[test]
fn should_not_defer_cleanup_when_cwd_at_repo_root() {
    let repo = TempDir::new().expect("tempdir");
    let repo_root = repo.path();
    make_worktree_dir(repo_root, "stage-1");

    assert!(!should_defer_cleanup(repo_root, repo_root, "stage-1"));
}

#[test]
fn should_not_defer_cleanup_when_cwd_in_different_stage_worktree() {
    let repo = TempDir::new().expect("tempdir");
    let repo_root = repo.path();
    make_worktree_dir(repo_root, "stage-1");
    let other_worktree = make_worktree_dir(repo_root, "stage-2");

    assert!(!should_defer_cleanup(&other_worktree, repo_root, "stage-1"));
}

#[test]
fn should_not_defer_cleanup_when_worktree_path_does_not_exist() {
    let repo = TempDir::new().expect("tempdir");
    let repo_root = repo.path();
    // stage-1's worktree was never created on disk.

    assert!(!should_defer_cleanup(repo_root, repo_root, "stage-1"));
}
