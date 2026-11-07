//! The static gate's cleanup: files its checks create must not move the tree
//! digest the freeze job compares before and after the run.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::*;
use crate::models::stage::Stage;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "-c", "user.name=t"])
        .args(["-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// A repository with a tracked `a.rs` and an ignore rule for `*.log`.
fn repository() -> (TempDir, WorktreeGit) {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    let repo = WorktreeGit::discovered(&dir);
    (temp, repo)
}

fn digest_of(repo: &WorktreeGit) -> String {
    digest::tree_digest_in(repo).unwrap()
}

#[test]
fn a_file_the_gate_created_is_removed_and_the_digest_is_unchanged() {
    let (_temp, repo) = repository();
    let root = repo.work_tree().to_path_buf();
    let before = digest::untracked_in(&repo).unwrap();
    let digest_before = digest_of(&repo);

    std::fs::write(root.join("tsconfig.tsbuildinfo"), "{}").unwrap();
    assert_ne!(
        digest_of(&repo),
        digest_before,
        "the stub's file moves the digest"
    );

    let removed = remove_created(&repo, &root, &before).unwrap();
    assert_eq!(removed, ["tsconfig.tsbuildinfo"]);
    assert!(!root.join("tsconfig.tsbuildinfo").exists());
    assert_eq!(digest_of(&repo), digest_before);
}

#[test]
fn files_present_before_the_gate_and_ignored_or_tracked_files_stay() {
    let (_temp, repo) = repository();
    let root = repo.work_tree().to_path_buf();
    std::fs::write(root.join("notes.txt"), "untracked before the gate").unwrap();
    let before = digest::untracked_in(&repo).unwrap();
    assert!(before.contains("notes.txt"), "{before:?}");

    std::fs::write(root.join("lint.log"), "ignored").unwrap();
    std::fs::write(root.join("a.rs"), "fn a() { /* edited by the gate */ }\n").unwrap();
    let removed = remove_created(&repo, &root, &before).unwrap();

    assert!(removed.is_empty(), "{removed:?}");
    assert!(
        root.join("notes.txt").is_file(),
        "untracked before the gate"
    );
    assert!(root.join("lint.log").is_file(), "git-ignored");
    assert!(
        std::fs::read_to_string(root.join("a.rs"))
            .unwrap()
            .contains("edited"),
        "a tracked file stays changed, so the binding check fails"
    );
}

#[test]
fn a_nested_file_is_removed_and_its_directory_stays() {
    let (_temp, repo) = repository();
    let root = repo.work_tree().to_path_buf();
    let before = digest::untracked_in(&repo).unwrap();
    std::fs::create_dir_all(root.join("target/cache")).unwrap();
    std::fs::write(root.join("target/cache/x.bin"), "x").unwrap();

    let removed = remove_created(&repo, &root, &before).unwrap();

    assert_eq!(removed, ["target/cache/x.bin"]);
    assert!(root.join("target/cache").is_dir());
}

/// A site whose only acceptance criterion is a pure `grep` that writes
/// `out.txt`, then fails naming `missing.txt` on stderr.
fn failing_site(root: &Path) -> ContractSite {
    let stage = Stage {
        id: "s1".to_string(),
        acceptance: vec![crate::models::stage::AcceptanceCriterion::Simple(
            "grep -n fn a.rs missing.txt > out.txt".to_string(),
        )],
        ..Stage::default()
    };
    ContractSite::resolved(
        root.join(".loom/work"),
        stage,
        root.to_path_buf(),
        root.to_path_buf(),
    )
}

#[test]
fn a_refusal_also_removes_the_files_the_gate_created() {
    let (_temp, repo) = repository();
    let root = repo.work_tree().to_path_buf();
    let digest_before = digest_of(&repo);
    let site = failing_site(&root);
    let written = ["missing.txt".to_string()];

    let outcome =
        format_outcome(&site, &repo, CommandConfinement::Inherit, &written, true).unwrap();

    assert!(!outcome.passed, "{}", outcome.summary);
    assert!(
        outcome.summary.contains("missing.txt"),
        "{}",
        outcome.summary
    );
    assert!(
        outcome
            .summary
            .contains("removed files the checks created:")
            && outcome.summary.contains("out.txt"),
        "{}",
        outcome.summary
    );
    assert!(!root.join("out.txt").exists());
    assert_eq!(digest_of(&repo), digest_before);
}

#[test]
fn a_failure_naming_no_written_file_passes_the_gate() {
    let (_temp, repo) = repository();
    let root = repo.work_tree().to_path_buf();
    let site = failing_site(&root);

    let outcome = format_outcome(&site, &repo, CommandConfinement::Inherit, &[], true).unwrap();

    assert!(outcome.passed, "{}", outcome.summary);
    assert!(
        outcome
            .summary
            .starts_with("every contract and harness file passes"),
        "{}",
        outcome.summary
    );
    assert!(!root.join("out.txt").exists());
}
