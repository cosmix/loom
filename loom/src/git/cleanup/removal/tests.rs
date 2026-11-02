use super::*;
use crate::git::cleanup::cleanup_after_merge;
use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
        .env("GIT_CONFIG_SYSTEM", root.join(".no-system-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "test@example.com"]);
    git_ok(root, &["config", "user.name", "Test"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git_ok(root, &["add", "README.md"]);
    git_ok(root, &["commit", "-m", "base"]);
    temp
}

fn create_stage_worktree(root: &Path, stage_id: &str) -> (PathBuf, String) {
    let worktrees = root.join(".worktrees");
    fs::create_dir_all(&worktrees).unwrap();
    let worktree = worktrees.join(stage_id);
    let worktree_arg = worktree.to_string_lossy().to_string();
    let branch = branch_name_for_stage(stage_id);
    git_ok(
        root,
        &["worktree", "add", "-b", &branch, &worktree_arg, "main"],
    );
    fs::write(worktree.join("feature.txt"), "feature\n").unwrap();
    git_ok(&worktree, &["add", "feature.txt"]);
    git_ok(&worktree, &["commit", "-m", "feature"]);
    let commit = git_stdout(&worktree, &["rev-parse", "HEAD"]);
    (worktree, commit)
}

fn merge_stage(root: &Path, stage_id: &str) {
    let branch = branch_name_for_stage(stage_id);
    git_ok(root, &["merge", "--no-ff", "-m", "merge stage", &branch]);
}

#[test]
fn verified_cleanup_reconciles_half_removed_worktree_and_reports_retained_path() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "retained");
    merge_stage(root, "retained");
    fs::remove_dir_all(&worktree).unwrap();
    fs::remove_dir_all(root.join(".git/worktrees/retained")).unwrap();
    fs::create_dir_all(worktree.join(".claude/.cc-writes")).unwrap();
    fs::write(root.join("README.md"), "operator edits in enclosing repo").unwrap();

    for _ in 0..2 {
        let result = cleanup_verified_stage("retained", &commit, "main", root).unwrap();
        assert!(!result.worktree_removed);
        assert!(result.warnings[0].contains(worktree.to_str().unwrap()));
        assert!(worktree.join(".claude/.cc-writes").is_dir());
    }
    assert_eq!(
        fs::read_to_string(root.join("README.md")).unwrap(),
        "operator edits in enclosing repo"
    );
}

#[test]
fn verified_cleanup_preserves_detached_admin_work_beyond_the_completed_commit() {
    let temp = init_repo();
    let root = temp.path();
    let completed = git_stdout(root, &["rev-parse", "HEAD"]);
    let (worktree, unmerged) = create_stage_worktree(root, "detached");
    git_ok(&worktree, &["checkout", "--detach"]);
    git_ok(root, &["branch", "-D", "loom/detached"]);
    let admin = root.join(".git/worktrees/detached");
    fs::remove_dir_all(&worktree).unwrap();
    fs::create_dir_all(worktree.join(".claude/.cc-writes")).unwrap();

    assert!(cleanup_verified_stage("detached", &completed, "main", root).is_err());

    assert_eq!(
        fs::read_to_string(admin.join("HEAD")).unwrap().trim(),
        unmerged
    );
    assert!(admin.join("index").is_file());
    assert!(worktree.join(".claude/.cc-writes").is_dir());
}

#[test]
fn verified_cleanup_refuses_modified_worktree() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "dirty");
    merge_stage(root, "dirty");
    fs::write(worktree.join("feature.txt"), "uncommitted\n").unwrap();

    let error = cleanup_verified_stage("dirty", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("uncommitted changes"));
    assert!(worktree.join("feature.txt").exists());
    assert!(branch_exists_strict("loom/dirty", root).unwrap());
}

#[test]
fn verified_cleanup_does_not_follow_a_foreign_gitdir() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "redirected");
    merge_stage(root, "redirected");
    let foreign = init_repo();
    let marker = worktree.join("filter-ran");
    fs::write(
        foreign.path().join(".gitattributes"),
        "feature.txt filter=probe\n",
    )
    .unwrap();
    fs::write(foreign.path().join("feature.txt"), "foreign\n").unwrap();
    git_ok(foreign.path(), &["add", ".gitattributes", "feature.txt"]);
    git_ok(foreign.path(), &["commit", "-m", "foreign"]);
    git_ok(
        foreign.path(),
        &["config", "filter.probe.clean", "touch filter-ran; cat"],
    );
    fs::write(
        worktree.join(".gitattributes"),
        "feature.txt filter=probe\n",
    )
    .unwrap();
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", foreign.path().join(".git").display()),
    )
    .unwrap();

    git_ok(&worktree, &["status", "--porcelain"]);
    assert!(
        marker.exists(),
        "foreign filter must execute in the positive control"
    );
    fs::remove_file(&marker).unwrap();

    assert!(cleanup_verified_stage("redirected", &commit, "main", root).is_err());
    assert!(!marker.exists(), "cleanup executed the foreign filter");
    assert!(worktree.exists());
}

#[test]
fn verified_cleanup_refuses_untracked_worktree_file() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "untracked");
    merge_stage(root, "untracked");
    fs::write(worktree.join("notes.txt"), "keep me\n").unwrap();

    let error = cleanup_verified_stage("untracked", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("notes.txt"));
    assert!(worktree.join("notes.txt").exists());
}

#[test]
fn verified_cleanup_refuses_unmerged_commit() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "unmerged");

    let error = cleanup_verified_stage("unmerged", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("not retained"));
    assert!(worktree.exists());
    assert!(branch_exists_strict("loom/unmerged", root).unwrap());
}

#[test]
fn verified_cleanup_refuses_unmerged_stage_base_branch() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "diverged-base");
    merge_stage(root, "diverged-base");
    git_ok(root, &["checkout", "-b", "loom/_base/diverged-base"]);
    fs::write(root.join("base-only.txt"), "not retained\n").unwrap();
    git_ok(root, &["add", "base-only.txt"]);
    git_ok(root, &["commit", "-m", "unmerged base change"]);
    git_ok(root, &["checkout", "main"]);

    let error = cleanup_verified_stage("diverged-base", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("stage base branch head"));
    assert!(worktree.exists());
    assert!(branch_exists_strict("loom/diverged-base", root).unwrap());
    assert!(base_branch_exists("diverged-base", root).unwrap());
}

#[test]
fn verified_cleanup_ignores_a_tag_shadowing_the_stage_branch() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "shadowed");
    merge_stage(root, "shadowed");
    // The branch gains an unmerged commit while a tag of the same name sits
    // at the merged commit; a bare branch name would resolve to the tag.
    fs::write(worktree.join("later.txt"), "unmerged\n").unwrap();
    git_ok(&worktree, &["add", "later.txt"]);
    git_ok(&worktree, &["commit", "-m", "unmerged work"]);
    git_ok(root, &["tag", "loom/shadowed", &commit]);

    let error = cleanup_verified_stage("shadowed", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains("stage branch head"));
    assert!(worktree.exists());
    assert!(branch_exists_strict("loom/shadowed", root).unwrap());
}

#[test]
fn verified_cleanup_refuses_when_both_resources_are_absent() {
    let temp = init_repo();
    let root = temp.path();
    let main_commit = git_stdout(root, &["rev-parse", "main"]);

    let error = cleanup_verified_stage("absent", &main_commit, "main", root).unwrap_err();
    assert!(error
        .to_string()
        .contains("refusing to infer that it was merged"));
}

#[test]
fn cleanup_failure_is_returned_as_an_error() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join(".worktrees/broken")).unwrap();
    let config = CleanupConfig {
        force_worktree_removal: false,
        force_branch_deletion: false,
        prune_worktrees: true,
        verbose: false,
    };

    let error = cleanup_after_merge("broken", root, &config).unwrap_err();
    assert!(error.to_string().contains("Failed to remove worktree"));
    assert!(root.join(".worktrees/broken").exists());
}

#[test]
fn verified_cleanup_removes_legitimately_merged_resources() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "merged");
    merge_stage(root, "merged");

    let result = cleanup_verified_stage("merged", &commit, "main", root).unwrap();
    assert!(result.worktree_removed);
    assert!(result.branch_deleted);
    assert!(!worktree.exists());
    assert!(!branch_exists_strict("loom/merged", root).unwrap());
}

#[test]
fn destructive_cleanup_requires_exact_confirmation() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage_worktree(root, "forced");

    let error = cleanup_destructive_stage("forced", "forced", root).unwrap_err();
    assert!(error.to_string().contains("exact confirmation"));
    assert!(worktree.exists());

    let confirmation = destructive_removal_confirmation("forced");
    let result = cleanup_destructive_stage("forced", &confirmation, root).unwrap();
    assert!(result.worktree_removed);
    assert!(result.branch_deleted);
}

#[test]
fn destructive_cleanup_rejects_stage_id_path_traversal() {
    let temp = init_repo();
    let root = temp.path();
    let confirmation = destructive_removal_confirmation("../outside");

    let error = cleanup_destructive_stage("../outside", &confirmation, root).unwrap_err();
    assert!(error.to_string().contains("Invalid stage ID"));
}

#[test]
fn verified_cleanup_removes_worktree_holding_ignored_output() {
    let temp = init_repo();
    let root = temp.path();
    fs::write(root.join(".gitignore"), "target/\n.claude/\n").unwrap();
    git_ok(root, &["add", ".gitignore"]);
    git_ok(root, &["commit", "-m", "ignore build output"]);
    let (worktree, commit) = create_stage_worktree(root, "ignored");
    merge_stage(root, "ignored");
    fs::create_dir_all(worktree.join("target/debug")).unwrap();
    fs::write(worktree.join("target/debug/app"), "bin").unwrap();
    fs::create_dir_all(worktree.join("src/.claude/.cc-writes")).unwrap();
    fs::write(worktree.join("src/.claude/.cc-writes/x"), "w").unwrap();

    let result = cleanup_verified_stage("ignored", &commit, "main", root).unwrap();
    assert!(result.worktree_removed);
    assert!(!worktree.exists());
    assert!(!branch_exists_strict("loom/ignored", root).unwrap());
}

#[test]
fn verified_cleanup_discounts_untracked_loom_scaffold() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "scaffold");
    merge_stage(root, "scaffold");
    fs::create_dir_all(worktree.join(".claude")).unwrap();
    fs::write(worktree.join(".claude/settings.json"), "{}").unwrap();

    cleanup_verified_stage("scaffold", &commit, "main", root).unwrap();
    assert!(!worktree.exists());
}

#[test]
fn verified_cleanup_removes_empty_sandbox_stubs() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "stubs");
    merge_stage(root, "stubs");
    fs::write(worktree.join(".bashrc"), "").unwrap();
    fs::write(worktree.join(".mcp.json"), "").unwrap();

    cleanup_verified_stage("stubs", &commit, "main", root).unwrap();
    assert!(!worktree.exists());
}

#[test]
fn verified_cleanup_refuses_sandbox_stub_name_with_content() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, commit) = create_stage_worktree(root, "stub-content");
    merge_stage(root, "stub-content");
    fs::write(worktree.join(".zshrc"), "alias x=y\n").unwrap();

    let error = cleanup_verified_stage("stub-content", &commit, "main", root).unwrap_err();
    assert!(error.to_string().contains(".zshrc"));
    assert!(worktree.join(".zshrc").exists());
    assert!(branch_exists_strict("loom/stub-content", root).unwrap());
}
