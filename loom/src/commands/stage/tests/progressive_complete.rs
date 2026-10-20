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
    attempt_progressive_merge, complete_with_merge, should_defer_cleanup, Landing, MergeOutcome,
};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, save_stage};

/// Build a real git repo with a `.loom/work` directory and a `config.toml` that
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

    // Create a minimal state directory with config.toml so `get_merge_point`
    // can resolve to "main".
    let work_dir = repo_root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).expect("mkdir .loom/work");
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
    let work_dir = repo_root.join(".loom").join("work");

    let mut stage = make_stage("stage-no-branch");
    assert!(!stage.merged, "precondition: stage starts unmerged");

    // No loom/stage-no-branch branch exists. The progressive merge should
    // refuse to mark the stage merged regardless of how the missing branch
    // surfaces (Blocked outcome, or an Err from the deeper git call).
    let outcome = attempt_progressive_merge(&mut stage, repo_root, &work_dir, Landing::Session);

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
        Ok(MergeOutcome::Conflict | MergeOutcome::Held | MergeOutcome::NoCommits) => {
            panic!("unexpected Conflict, Held or NoCommits from missing branch");
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

/// A stage branch `loom/<id>` one commit ahead of `main`, adding
/// `.claude/settings.json`, and the stage saved as `Executing`.
fn control_path_stage(repo_root: &std::path::Path, id: &str) -> Stage {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    };
    git(&["checkout", "-q", "-b", &format!("loom/{id}")]);
    std::fs::create_dir_all(repo_root.join(".claude")).expect("mkdir .claude");
    std::fs::write(repo_root.join(".claude/settings.json"), "{}\n").expect("write settings");
    git(&["add", ".claude/settings.json"]);
    git(&["commit", "-q", "-m", "stage work"]);
    git(&["checkout", "-q", "main"]);
    let mut stage = make_stage(id);
    stage.completed_commit =
        Some(crate::git::get_branch_head(&format!("loom/{id}"), repo_root).unwrap());
    save_stage(&stage, &repo_root.join(".loom").join("work")).expect("save stage");
    stage
}

#[test]
fn complete_with_merge_holds_a_control_path_branch_for_review() {
    let repo = init_repo_with_work_dir();
    let repo_root = repo.path();
    let work_dir = repo_root.join(".loom").join("work");
    let mut stage = control_path_stage(repo_root, "ctl-hold");

    let error = complete_with_merge(&mut stage, repo_root, &work_dir, Landing::Session)
        .expect_err("a control-path branch must not complete");

    assert!(error.to_string().contains("control path"), "{error}");
    let on_disk = load_stage("ctl-hold", &work_dir).unwrap();
    assert_eq!(on_disk.status, StageStatus::NeedsHumanReview);
    assert!(!on_disk.merged);
    assert!(on_disk
        .review_reason
        .unwrap()
        .contains(".claude/settings.json"));
    assert!(!repo_root.join(".claude").exists(), "nothing was merged");
}

#[test]
fn complete_with_merge_bypass_lands_a_control_path_branch() {
    let repo = init_repo_with_work_dir();
    let repo_root = repo.path();
    let work_dir = repo_root.join(".loom").join("work");
    let mut stage = control_path_stage(repo_root, "ctl-bypass");
    stage.status = StageStatus::NeedsHumanReview;
    crate::verify::transitions::update_stage("ctl-bypass", &work_dir, |s| {
        s.status = StageStatus::NeedsHumanReview;
        Ok(())
    })
    .expect("park the stage for review");

    let completed = complete_with_merge(&mut stage, repo_root, &work_dir, Landing::Reviewed)
        .expect("the operator's override merges");

    assert!(completed);
    let on_disk = load_stage("ctl-bypass", &work_dir).unwrap();
    assert_eq!(on_disk.status, StageStatus::Completed);
    assert!(on_disk.merged);
    assert!(repo_root.join(".claude/settings.json").exists());
}

/// A stage branch `loom/<id>` at the tip of `main`, with no commit of its own,
/// and the stage saved as `Executing`.
fn zero_commit_stage(repo_root: &std::path::Path, id: &str) -> Stage {
    let out = Command::new("git")
        .args(["branch", &format!("loom/{id}")])
        .current_dir(repo_root)
        .output()
        .expect("git branch");
    assert!(out.status.success(), "git branch");
    let stage = make_stage(id);
    save_stage(&stage, &repo_root.join(".loom").join("work")).expect("save stage");
    stage
}

#[test]
fn complete_with_merge_routes_a_zero_commit_branch_to_review() {
    let repo = init_repo_with_work_dir();
    let repo_root = repo.path();
    let work_dir = repo_root.join(".loom").join("work");
    let mut stage = zero_commit_stage(repo_root, "zero");

    let error = complete_with_merge(&mut stage, repo_root, &work_dir, Landing::Session)
        .expect_err("a branch without commits must not complete");

    assert!(error.to_string().contains("no commits"), "{error}");
    let on_disk = load_stage("zero", &work_dir).unwrap();
    assert_eq!(on_disk.status, StageStatus::NeedsHumanReview);
    assert!(!on_disk.merged);
    assert!(on_disk
        .review_reason
        .unwrap()
        .contains("zero commits beyond main"));
    assert!(!stage.merged);
}

#[test]
fn a_zero_commit_branch_whose_recorded_commit_is_in_the_target_is_not_held() {
    let repo = init_repo_with_work_dir();
    let repo_root = repo.path();
    let mut stage = zero_commit_stage(repo_root, "landed");
    stage.completed_commit =
        Some(crate::git::runner::run_git_checked(&["rev-parse", "main"], repo_root).unwrap());

    assert!(
        super::super::progressive_complete::zero_commit_review_reason(&stage, "main", repo_root)
            .is_none()
    );
}
