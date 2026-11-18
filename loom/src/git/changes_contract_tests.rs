//! Contract tests for the shared merge base and change listing.
//!
//! Each fixture is a TempDir repository built with the shared `git` helpers, isolated
//! from the caller's environment and global configuration, so the listings
//! under test see exactly the state each scenario describes.

use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

use crate::git::changes::{changed_names, changes_since, merge_base, DiffNames};
use crate::git::worktree::WorktreeGit;
use crate::test_git::{git, TestRepo};

/// A repository with a local identity and no commit, on branch `branch`.
fn init_repo(branch: &str) -> TestRepo {
    TestRepo::builder().branch(branch).no_seed().create()
}

fn names(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

#[test]
fn merge_base_errors_without_shared_history() {
    let repo = init_repo("a");
    repo.commit_file("a.txt", "a\n", "a.txt");
    git(repo.path(), &["checkout", "-q", "--orphan", "b"]);
    git(repo.path(), &["rm", "-q", "-r", "--cached", "."]);
    std::fs::remove_file(repo.path().join("a.txt")).expect("remove a.txt");
    repo.commit_file("b.txt", "b\n", "b.txt");

    let base = merge_base(&WorktreeGit::discovered(repo.path()), "a", "b");

    assert!(
        base.is_err(),
        "branches with no shared history have no merge base, got {base:?}"
    );
}

#[test]
fn rename_lists_both_paths() {
    let repo = init_repo("main");
    let base = repo.commit_file("a.rs", "fn moved() {}\n", "a.rs");
    git(repo.path(), &["mv", "a.rs", "b.rs"]);
    git(repo.path(), &["commit", "-q", "-m", "move"]);

    let listed = changes_since(&WorktreeGit::discovered(repo.path()), &base, None)
        .expect("listing succeeds");

    assert_eq!(listed, names(&["a.rs", "b.rs"]));
}

#[test]
fn changes_since_skips_scaffold() {
    let repo = init_repo("main");
    let base = repo.commit_file("seed.txt", "seed\n", "seed.txt");
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
    repo.commit_file(" lead.rs", "fn lead() {}\n", " lead.rs");
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
    repo.commit_file("a.rs", "fn a() {}\n", "a.rs");
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
