//! Integration tests for the phantom-merge prevention work in
//! `doc/plans/PLAN-fix-phantom-merge.md`.
//!
//! A phantom merge is a stage marked `merged: true` whose `completed_commit`
//! is NOT an ancestor of the target branch. That state silently satisfies
//! downstream dependency checks and causes lost work (the user-reported
//! `oauth-hardening` incident).
//!
//! These tests construct phantom-merge states by hand, then exercise the
//! full fix surface:
//!
//! * `loom repair` detects a phantom merge as `Severity::Critical` (Fix 13).
//! * `loom repair --fix` reverts `merged` to false for the phantom case.
//! * A `Completed + !merged + !completed_commit` stage with a missing branch
//!   is left alone by the repair path and visible for manual intervention.
//!
//! Each test documents which plan fix it guards against regression for.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serial_test::serial;
use tempfile::TempDir;

use loom::commands::repair;
use loom::models::stage::{Stage, StageStatus, StageType};
use loom::verify::transitions::{load_stage, save_stage};

/// Build a real git repo on branch `main` with an initial commit. Returns the
/// TempDir (callers keep it alive for the duration of the test).
fn init_repo() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    run_git(&["init", "-b", "main"], root);
    run_git(&["config", "user.email", "test@test.com"], root);
    run_git(&["config", "user.name", "Test"], root);
    fs::write(root.join("README.md"), "initial\n").expect("write README");
    run_git(&["add", "README.md"], root);
    run_git(&["commit", "-m", "initial"], root);
    run_git(&["branch", "-M", "main"], root);

    tmp
}

fn run_git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Create a branch, add one commit, return the commit SHA. Leaves caller on `main`.
fn create_loom_branch_with_commit(
    stage_id: &str,
    filename: &str,
    content: &str,
    repo_root: &Path,
) -> String {
    let branch = format!("loom/{stage_id}");
    run_git(&["checkout", "-b", &branch], repo_root);
    fs::write(repo_root.join(filename), content).expect("write file");
    run_git(&["add", filename], repo_root);
    run_git(&["commit", "-m", "stage work"], repo_root);

    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .unwrap();
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();

    run_git(&["checkout", "main"], repo_root);
    sha
}

/// Create an empty `.work/` with a minimal `config.toml` pointing at `main`.
fn init_work_dir(repo_root: &Path) -> PathBuf {
    let work_dir = repo_root.join(".work");
    fs::create_dir_all(work_dir.join("stages")).expect("mkdir .work/stages");
    fs::write(work_dir.join("config.toml"), "base_branch = \"main\"\n").expect("write config.toml");
    work_dir
}

/// Build a Stage in the given Completed/merged state and write it to disk.
fn write_phantom_stage(
    stage_id: &str,
    merged: bool,
    completed_commit: Option<String>,
    work_dir: &Path,
) {
    let mut stage = Stage::new(stage_id.to_string(), Some(format!("test {stage_id}")));
    stage.id = stage_id.to_string();
    stage.stage_type = StageType::Standard;
    stage.status = StageStatus::Completed;
    stage.completed_at = Some(chrono::Utc::now());
    stage.merged = merged;
    stage.completed_commit = completed_commit;
    save_stage(&stage, work_dir).expect("save stage");
}

/// Guarded cwd change: restores the prior cwd even if the closure panics.
/// We use #[serial] on tests that enter this helper to avoid races.
fn with_cwd<F: FnOnce()>(dir: &Path, f: F) {
    let prior = std::env::current_dir().expect("getcwd");
    std::env::set_current_dir(dir).expect("set cwd to test repo");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::env::set_current_dir(&prior).expect("restore cwd");
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Fix 13 (detect) + Fix 13 (revert):
/// A phantom merge constructed by hand (merged=true, commit exists on its
/// branch but is NOT in main) is detected AND reverted by `loom repair --fix`.
///
/// This is the direct regression guard against the user-reported
/// `oauth-hardening` lost-work incident.
#[test]
#[serial]
fn repair_fix_reverts_phantom_merge_flag() {
    let repo = init_repo();
    let repo_root = repo.path();
    let work_dir = init_work_dir(repo_root);

    // Create loom/oauth-hardening with one commit, never merge it into main.
    let stranded_sha = create_loom_branch_with_commit(
        "oauth-hardening",
        "oauth.rs",
        "hardened auth flow",
        repo_root,
    );

    // Write stage state as if the daemon force-wrote merged=true.
    write_phantom_stage(
        "oauth-hardening",
        true,
        Some(stranded_sha.clone()),
        &work_dir,
    );

    // Precondition: on disk the stage is phantom-merged.
    let before = load_stage("oauth-hardening", &work_dir).expect("load before");
    assert!(before.merged, "precondition: stage file starts merged=true");

    // Run `loom repair --fix` from the repo root.
    with_cwd(repo_root, || {
        repair::execute(true).expect("repair --fix should succeed");
    });

    // After the fix, the stage's merged flag must be reverted to false.
    let after = load_stage("oauth-hardening", &work_dir).expect("load after");
    assert!(
        !after.merged,
        "Fix 13: repair --fix must revert merged flag for phantom merge \
         (was merged=true with commit not in main)"
    );
    assert_eq!(
        after.completed_commit,
        Some(stranded_sha),
        "repair must NOT clobber completed_commit (user may need it to cherry-pick)"
    );
}

/// Fix 13 (dry-run): without `--fix`, repair leaves the phantom merge in place
/// on disk. This preserves the user's ability to inspect before committing
/// to the revert.
#[test]
#[serial]
fn repair_dry_run_does_not_modify_phantom_merge() {
    let repo = init_repo();
    let repo_root = repo.path();
    let work_dir = init_work_dir(repo_root);

    let stranded_sha = create_loom_branch_with_commit("feat-a", "a.rs", "a", repo_root);
    write_phantom_stage("feat-a", true, Some(stranded_sha), &work_dir);

    with_cwd(repo_root, || {
        repair::execute(false).expect("repair (dry-run) should succeed");
    });

    let after = load_stage("feat-a", &work_dir).expect("load after");
    assert!(
        after.merged,
        "dry-run must NOT mutate disk — phantom merge flag should remain until --fix is used"
    );
}

/// Fix 13 happy path: a stage whose completed_commit IS in main is NOT
/// flagged as a phantom merge, so repair leaves it alone.
#[test]
#[serial]
fn repair_does_not_flag_legitimately_merged_stage() {
    let repo = init_repo();
    let repo_root = repo.path();
    let work_dir = init_work_dir(repo_root);

    let sha = create_loom_branch_with_commit("legit-stage", "ok.rs", "ok", repo_root);
    // Merge it into main for real.
    run_git(
        &["merge", "--no-ff", "-m", "merge legit", "loom/legit-stage"],
        repo_root,
    );

    write_phantom_stage("legit-stage", true, Some(sha.clone()), &work_dir);

    with_cwd(repo_root, || {
        repair::execute(true).expect("repair --fix should succeed");
    });

    let after = load_stage("legit-stage", &work_dir).expect("load after");
    assert!(
        after.merged,
        "legitimately-merged stage must retain merged=true after repair --fix"
    );
    assert_eq!(
        after.completed_commit,
        Some(sha),
        "completed_commit must be preserved"
    );
}

/// Fix 13 exemption: knowledge stages legitimately have merged=true with no
/// completed_commit (no branch by design). Repair must NOT flag them and must
/// NOT touch their state.
#[test]
#[serial]
fn repair_skips_knowledge_stages() {
    let repo = init_repo();
    let repo_root = repo.path();
    let work_dir = init_work_dir(repo_root);

    let mut stage = Stage::new("kb".to_string(), Some("knowledge bootstrap".to_string()));
    stage.id = "kb".to_string();
    stage.stage_type = StageType::Knowledge;
    stage.status = StageStatus::Completed;
    stage.completed_at = Some(chrono::Utc::now());
    stage.merged = true;
    stage.completed_commit = None;
    save_stage(&stage, &work_dir).expect("save knowledge stage");

    with_cwd(repo_root, || {
        repair::execute(true).expect("repair --fix should succeed");
    });

    let after = load_stage("kb", &work_dir).expect("load after");
    assert!(
        after.merged,
        "knowledge stage merged=true is legitimate — repair must not revert it"
    );
    assert_eq!(after.stage_type, StageType::Knowledge);
    assert!(after.completed_commit.is_none());
}

/// Fix 13 warn-only path: a stage with Completed + !merged and a missing
/// loom branch is flagged as stale but NOT auto-fixed (fix_issue returns
/// false for the "Stale:" branch). We assert the stage state is preserved
/// so the user can investigate and decide manually.
#[test]
#[serial]
fn repair_leaves_stale_completed_unmerged_stage_untouched() {
    let repo = init_repo();
    let repo_root = repo.path();
    let work_dir = init_work_dir(repo_root);

    // No loom/ghost branch created — simulates a stage whose branch was
    // cleaned up without a merge record.
    write_phantom_stage("ghost", false, None, &work_dir);

    with_cwd(repo_root, || {
        repair::execute(true).expect("repair --fix should succeed");
    });

    let after = load_stage("ghost", &work_dir).expect("load after");
    assert!(
        !after.merged,
        "stale Completed + !merged stage must remain unmerged (repair only warns)"
    );
    assert_eq!(
        after.status,
        StageStatus::Completed,
        "status must be preserved — repair flags but does not auto-transition"
    );
}
