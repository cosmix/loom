//! Real-git regression tests for `are_all_dependencies_satisfied` — the
//! phantom-merge prevention path added in `PLAN-fix-phantom-merge.md` (Fix 9).
//!
//! The metadata-only tests in `dependency_satisfaction.rs` set
//! `stage_type = StageType::Knowledge` to bypass the git ancestry check. These
//! tests exercise the opposite path: a real git repository with standard (non-
//! knowledge) dependency stages, where `merged = true` alone must NOT satisfy a
//! dependent when the dep's `completed_commit` is not an ancestor of the
//! target branch.
//!
//! Each test explicitly documents which fix it guards.

use std::process::Command;

use serial_test::serial;
use tempfile::TempDir;

use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::{are_all_dependencies_satisfied, save_stage};

use super::create_test_stage;

/// Initialize a real temporary git repo with an initial commit on `main`.
/// Returns the TempDir so callers can keep it alive for the duration of the test.
fn init_repo_with_main() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
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

    std::fs::write(repo_root.join("README.md"), "initial\n").expect("write README");

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

    // Ensure we are on main (older git defaults to 'master').
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(repo_root)
        .output()
        .expect("rename to main");

    temp_dir
}

/// Create a branch at the current HEAD, add a commit with a new file, and
/// return the SHA of that new commit. Leaves the caller on `main` when done.
fn create_branch_with_commit(
    branch: &str,
    filename: &str,
    content: &str,
    repo_root: &std::path::Path,
) -> String {
    Command::new("git")
        .args(["checkout", "-b", branch])
        .current_dir(repo_root)
        .output()
        .expect("checkout -b");

    std::fs::write(repo_root.join(filename), content).expect("write file");

    Command::new("git")
        .args(["add", filename])
        .current_dir(repo_root)
        .output()
        .expect("git add");

    Command::new("git")
        .args(["commit", "-m", &format!("add {filename}")])
        .current_dir(repo_root)
        .output()
        .expect("git commit");

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .expect("rev-parse");
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Return to main for isolation
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("checkout main");

    sha
}

/// Merge `branch` into `main` with --no-ff so the resulting main HEAD
/// incorporates the branch's commits.
fn merge_branch_into_main(branch: &str, repo_root: &std::path::Path) {
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .output()
        .expect("checkout main");

    Command::new("git")
        .args(["merge", "--no-ff", "-m", &format!("merge {branch}"), branch])
        .current_dir(repo_root)
        .output()
        .expect("git merge");
}

/// Fix 9: phantom-merge detection in `are_all_dependencies_satisfied`.
///
/// Two stages. The dep has `merged = true` and a valid `completed_commit`,
/// but that commit lives only on the stage branch — it was never merged
/// into `main`. `are_all_dependencies_satisfied` MUST return false.
#[test]
#[serial]
fn phantom_merge_commit_not_in_target_returns_false() {
    let repo = init_repo_with_main();
    let repo_root = repo.path();
    let work_dir = repo_root; // treat repo root as work_dir for this test

    // Create loom/dep-stage branch and a commit that stays off main.
    let dep_sha = create_branch_with_commit("loom/dep-stage", "dep.txt", "dep work", repo_root);

    // Dep stage as if daemon force-wrote merged=true (the phantom merge bug).
    let mut dep = create_test_stage("dep-stage", "Dep", StageStatus::Completed);
    dep.stage_type = StageType::Standard;
    dep.merged = true;
    dep.completed_commit = Some(dep_sha);
    save_stage(&dep, work_dir).expect("save dep");

    // Dependent stage.
    let mut dependent =
        create_test_stage("dependent-stage", "Dependent", StageStatus::WaitingForDeps);
    dependent.stage_type = StageType::Standard;
    dependent.add_dependency("dep-stage".to_string());

    let satisfied = are_all_dependencies_satisfied(&dependent, work_dir, repo_root, "main")
        .expect("ancestry check");
    assert!(
        !satisfied,
        "Phantom merge must NOT satisfy dependency: dep commit is not on main"
    );
}

/// Fix 9: when the dep commit IS in the target branch, dependencies are satisfied.
///
/// This is the happy path that the ancestry check must still allow through.
#[test]
#[serial]
fn real_merge_commit_in_target_returns_true() {
    let repo = init_repo_with_main();
    let repo_root = repo.path();
    let work_dir = repo_root;

    let dep_sha = create_branch_with_commit("loom/dep-stage", "dep.txt", "dep work", repo_root);
    merge_branch_into_main("loom/dep-stage", repo_root);

    let mut dep = create_test_stage("dep-stage", "Dep", StageStatus::Completed);
    dep.stage_type = StageType::Standard;
    dep.merged = true;
    dep.completed_commit = Some(dep_sha);
    save_stage(&dep, work_dir).expect("save dep");

    let mut dependent =
        create_test_stage("dependent-stage", "Dependent", StageStatus::WaitingForDeps);
    dependent.stage_type = StageType::Standard;
    dependent.add_dependency("dep-stage".to_string());

    let satisfied = are_all_dependencies_satisfied(&dependent, work_dir, repo_root, "main")
        .expect("ancestry check");
    assert!(
        satisfied,
        "Dependency with commit actually merged into main must be satisfied"
    );
}

/// Fix 9 exemption: knowledge-stage dependencies never need ancestry proof.
///
/// A knowledge dep has `merged = true` and no `completed_commit` (by design —
/// no branch). The ancestry check must be skipped and the dependency treated
/// as satisfied.
#[test]
#[serial]
fn knowledge_dep_with_no_commit_returns_true() {
    let repo = init_repo_with_main();
    let repo_root = repo.path();
    let work_dir = repo_root;

    let mut dep = create_test_stage("knowledge-dep", "Knowledge Dep", StageStatus::Completed);
    dep.stage_type = StageType::Knowledge;
    dep.merged = true;
    dep.completed_commit = None;
    save_stage(&dep, work_dir).expect("save knowledge dep");

    let mut dependent =
        create_test_stage("dependent-stage", "Dependent", StageStatus::WaitingForDeps);
    dependent.stage_type = StageType::Standard;
    dependent.add_dependency("knowledge-dep".to_string());

    let satisfied = are_all_dependencies_satisfied(&dependent, work_dir, repo_root, "main")
        .expect("ancestry check");
    assert!(
        satisfied,
        "Knowledge dep with merged=true and no commit must satisfy dependency (exemption preserved)"
    );
}

/// Fix 9: standard dep with `completed_commit: None` must NOT satisfy.
///
/// Without a commit we cannot verify ancestry. The contract says: log an error
/// and refuse to satisfy. This test asserts the refusal (the log side-effect
/// is not asserted here; functional behavior is what matters for regression).
#[test]
#[serial]
fn standard_dep_with_no_completed_commit_returns_false() {
    let repo = init_repo_with_main();
    let repo_root = repo.path();
    let work_dir = repo_root;

    let mut dep = create_test_stage("dep-stage", "Dep", StageStatus::Completed);
    dep.stage_type = StageType::Standard;
    dep.merged = true;
    dep.completed_commit = None; // the dangerous case
    save_stage(&dep, work_dir).expect("save dep");

    let mut dependent =
        create_test_stage("dependent-stage", "Dependent", StageStatus::WaitingForDeps);
    dependent.stage_type = StageType::Standard;
    dependent.add_dependency("dep-stage".to_string());

    let satisfied = are_all_dependencies_satisfied(&dependent, work_dir, repo_root, "main")
        .expect("ancestry check");
    assert!(
        !satisfied,
        "Standard dep without completed_commit must NOT satisfy dependency (cannot verify)"
    );
}
