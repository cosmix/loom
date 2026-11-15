//! Contract tests for the shared merge base and change listing.
//!
//! Each fixture is a TempDir repository built here with plain `git`, isolated
//! from the caller's environment and global configuration, so the listings
//! under test see exactly the state each scenario describes.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

use crate::git::changes::{changed_names, changes_since, merge_base, DiffNames};
use crate::git::worktree::WorktreeGit;

/// Run `git` in `dir` for fixture setup, failing the test on any error.
/// Returns stdout with the trailing newline removed.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf-8 git output")
        .trim_end_matches('\n')
        .to_string()
}

/// A repository with a local identity and no commit, on branch `branch`.
fn init_repo(branch: &str) -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", branch]);
    git(dir.path(), &["config", "user.name", "Contract Test"]);
    git(
        dir.path(),
        &["config", "user.email", "contract@example.invalid"],
    );
    dir
}

/// Write `name` with `contents`, stage it and commit; returns the new HEAD id.
fn commit_file(dir: &Path, name: &str, contents: &str) -> String {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(&path, contents).expect("write file");
    git(dir, &["add", "--", name]);
    git(dir, &["commit", "-q", "-m", name]);
    git(dir, &["rev-parse", "HEAD"])
}

fn names(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

#[test]
fn merge_base_errors_without_shared_history() {
    let repo = init_repo("a");
    commit_file(repo.path(), "a.txt", "a\n");
    git(repo.path(), &["checkout", "-q", "--orphan", "b"]);
    git(repo.path(), &["rm", "-q", "-r", "--cached", "."]);
    std::fs::remove_file(repo.path().join("a.txt")).expect("remove a.txt");
    commit_file(repo.path(), "b.txt", "b\n");

    let base = merge_base(&WorktreeGit::discovered(repo.path()), "a", "b");

    assert!(
        base.is_err(),
        "branches with no shared history have no merge base, got {base:?}"
    );
}

#[test]
fn rename_lists_both_paths() {
    let repo = init_repo("main");
    let base = commit_file(repo.path(), "a.rs", "fn moved() {}\n");
    git(repo.path(), &["mv", "a.rs", "b.rs"]);
    git(repo.path(), &["commit", "-q", "-m", "move"]);

    let listed = changes_since(&WorktreeGit::discovered(repo.path()), &base, None)
        .expect("listing succeeds");

    assert_eq!(listed, names(&["a.rs", "b.rs"]));
}

#[test]
fn changes_since_skips_scaffold() {
    let repo = init_repo("main");
    let base = commit_file(repo.path(), "seed.txt", "seed\n");
    std::fs::create_dir_all(repo.path().join(".claude")).expect("mkdir .claude");
    std::fs::write(repo.path().join(".claude/settings.json"), "{}\n").expect("write settings");
    std::fs::create_dir_all(repo.path().join("src")).expect("mkdir src");
    std::fs::write(repo.path().join("src/new.rs"), "fn new() {}\n").expect("write new.rs");

    let listed = changes_since(&WorktreeGit::discovered(repo.path()), &base, None)
        .expect("listing succeeds");

    assert_eq!(listed, names(&["src/new.rs"]));
}

#[test]
fn listing_is_untrimmed() {
    let repo = init_repo("main");
    commit_file(repo.path(), " lead.rs", "fn lead() {}\n");
    std::fs::write(repo.path().join(" lead.rs"), "fn lead() { () }\n").expect("modify file");
    let query = DiffNames {
        from: "HEAD",
        to: None,
        filter: None,
        all_submodules: false,
        pathspecs: &[],
    };

    let listed =
        changed_names(&WorktreeGit::discovered(repo.path()), &query).expect("listing succeeds");

    assert_eq!(listed, names(&[" lead.rs"]));
}

#[test]
fn listing_writes_no_index() {
    let repo = init_repo("main");
    commit_file(repo.path(), "a.rs", "fn a() {}\n");
    let file = std::fs::File::options()
        .write(true)
        .open(repo.path().join("a.rs"))
        .expect("open a.rs");
    let later = SystemTime::now() + Duration::from_secs(3600);
    file.set_modified(later).expect("set mtime");
    drop(file);
    let index = repo.path().join(".git/index");
    let before = std::fs::read(&index).expect("read index before");
    let query = DiffNames {
        from: "HEAD",
        to: None,
        filter: None,
        all_submodules: false,
        pathspecs: &[],
    };

    let listed =
        changed_names(&WorktreeGit::discovered(repo.path()), &query).expect("listing succeeds");

    let after = std::fs::read(&index).expect("read index after");
    assert_eq!(listed, BTreeSet::new(), "a stat-dirty file is not a change");
    assert!(
        before == after,
        "listing changes must not rewrite .git/index"
    );
}
