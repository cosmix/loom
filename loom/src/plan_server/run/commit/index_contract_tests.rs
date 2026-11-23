//! Contract: a handoff commit that a hook refuses leaves the staged index as it found it.
//! Pins the unchanged `git ls-files --stage` and `git diff --cached` after a refusal, with a
//! tracked ticked path that is partly staged, an untracked ticked path and an unrelated staged
//! change. Rejects a commit step that stages the whole work-tree content of a tracked path with
//! `git add` and only unstages the new file after the refusal.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::commit_doc_changes;
use crate::plan_server::run::changes::split_changes;
use crate::test_git::{git_checked, TestRepo};

fn git_text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git_checked(root, args).stdout).unwrap()
}

fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A repository whose hooks live in its own directory, so the user's hooks never run.
fn repo_with_hooks() -> TestRepo {
    let repo = TestRepo::builder()
        .identity("Index Contract", "index-contract@example.invalid")
        .no_seed()
        .create();
    let root = repo.path();
    let hooks = root.join("hooks");
    fs::create_dir(&hooks).unwrap();
    git_checked(root, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
    put(root, "doc/a.md", "original\n");
    put(root, "src.txt", "original\n");
    git_checked(root, &["add", "--", "doc/a.md", "src.txt"]);
    git_checked(root, &["commit", "-q", "-m", "initial"]);
    repo
}

/// What the index holds, as git reports it: the staged entries and the staged diff.
fn staged_state(root: &Path) -> (String, String) {
    let entries = git_text(root, &["ls-files", "--stage"]);
    let diff = git_text(root, &["diff", "--cached"]);
    (entries, diff)
}

#[test]
#[serial_test::serial]
fn a_refused_commit_leaves_the_staged_index_unchanged() {
    let repo = repo_with_hooks();
    let root = repo.path();
    put(root, "doc/a.md", "staged\n");
    git_checked(root, &["add", "--", "doc/a.md"]);
    put(root, "doc/a.md", "worktree\n");
    put(root, "doc/new.md", "new\n");
    put(root, "src.txt", "staged elsewhere\n");
    git_checked(root, &["add", "--", "src.txt"]);
    let hook = root.join("hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let before = staged_state(root);
    let head = git_text(root, &["rev-parse", "HEAD"]);

    let (current, _) = split_changes(root).unwrap();
    let paths = ["doc/a.md".to_owned(), "doc/new.md".to_owned()];
    let code = commit_doc_changes(root, &paths, "edit", &current, &mut |_| {}).unwrap();

    assert_eq!(code, 1, "the hook refuses the commit");
    assert_eq!(git_text(root, &["rev-parse", "HEAD"]), head);
    let after = staged_state(root);
    assert_eq!(after.0, before.0, "git ls-files --stage changed");
    assert_eq!(after.1, before.1, "git diff --cached changed");
}
