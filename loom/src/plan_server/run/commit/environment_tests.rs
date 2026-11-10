//! A commit's git steps name the server's repository and read its pathspecs
//! literally, whatever repository or pathspec variables the shell that started
//! the server exported; the user's hooks see none of loom's pathspec options.

use std::path::Path;
use std::process::Command;

use super::{strings, Repo};
use crate::plan_server::wire::run::ChangedPath;

const ROOT_ENV: &str = "LOOM_TEST_COMMIT_ROOT";
const PATHSPEC_ROOT_ENV: &str = "LOOM_TEST_PATHSPEC_ROOT";
/// A pre-commit hook that records the pathspec variables it sees.
const PATHSPEC_HOOK: &str = "#!/bin/sh\n\
    echo \"literal=${GIT_LITERAL_PATHSPECS:-unset} icase=${GIT_ICASE_PATHSPECS:-unset}\" > hook-env\n";
const UNSET: &str = "literal=unset icase=unset\n";

#[test]
#[serial_test::serial]
fn the_hooks_see_no_literal_pathspecs() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    repo.hook("pre-commit", PATHSPEC_HOOK);
    assert_eq!(repo.commit(&["doc/a.md"], "edit").unwrap(), 0);
    let seen = std::fs::read_to_string(repo.0.path().join("hook-env")).unwrap();
    assert_eq!(seen, UNSET);
}

/// Runs in a child, since the variables have to be in the whole process environment the steps
/// inherit. A literal pathspec under an inherited `GIT_LITERAL_PATHSPECS=1` would name a file
/// called `:(literal)doc/a*.md`.
#[test]
#[serial_test::serial]
fn a_commit_ignores_inherited_pathspec_variables() {
    let path = "doc/a*.md";
    if let Some(root) = std::env::var_os(PATHSPEC_ROOT_ENV) {
        commit_in_child(Path::new(&root), path);
        return;
    }
    let repo = Repo::new();
    repo.put(path, "selected\n");
    repo.put("doc/ab.md", "unticked\n");
    repo.hook("pre-commit", PATHSPEC_HOOK);

    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "plan_server::run::commit::tests::environment::\
             a_commit_ignores_inherited_pathspec_variables",
            "--nocapture",
        ])
        .env(PATHSPEC_ROOT_ENV, repo.0.path())
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_ICASE_PATHSPECS", "1")
        .status()
        .unwrap();

    assert!(
        status.success(),
        "the commit followed the inherited pathspec variables"
    );
    assert_eq!(
        repo.git(&["show", "--format=", "--name-only", "HEAD"]),
        format!("{path}\n")
    );
    assert_eq!(repo.git(&["ls-files", "--", "doc/ab.md"]), "");
    let seen = std::fs::read_to_string(repo.0.path().join("hook-env")).unwrap();
    assert_eq!(seen, UNSET);
}

/// Runs in a child, since the redirect has to be in the whole process
/// environment the steps inherit and other tests also run git.
#[test]
#[serial_test::serial]
fn a_commit_ignores_an_inherited_repository_redirect() {
    if let Some(root) = std::env::var_os(ROOT_ENV) {
        let current = [ChangedPath {
            path: "doc/a.md".to_owned(),
            status: "M".to_owned(),
        }];
        let code = super::commit_doc_changes(
            Path::new(&root),
            &strings(&["doc/a.md"]),
            "edit",
            &current,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(code, 0);
        return;
    }
    let repo = Repo::new();
    let other = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let other_head = other.git(&["rev-parse", "HEAD"]);

    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "plan_server::run::commit::tests::environment::\
             a_commit_ignores_an_inherited_repository_redirect",
            "--nocapture",
        ])
        .env(ROOT_ENV, repo.0.path())
        .env("GIT_DIR", other.0.path().join(".git"))
        .env("GIT_WORK_TREE", other.0.path())
        .env("GIT_INDEX_FILE", other.0.path().join(".git/index"))
        .status()
        .unwrap();

    assert!(status.success(), "the commit followed GIT_DIR");
    assert_eq!(other.git(&["rev-parse", "HEAD"]), other_head);
    assert_ne!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "edit\n");
}

/// The child's half: commit the new file at `path` in `root`, the index reset after it.
fn commit_in_child(root: &Path, path: &str) {
    let current = [ChangedPath {
        path: path.to_owned(),
        status: "??".to_owned(),
    }];
    let mut lines = Vec::new();
    let code = super::commit_doc_changes(
        root,
        &strings(&[path]),
        "add literal path",
        &current,
        &mut |line| lines.push(line.to_owned()),
    )
    .unwrap();
    assert_eq!(code, 0, "{lines:?}");
    let reset = lines.iter().any(|line| line.contains("could not be reset"));
    assert!(!reset, "{lines:?}");
}
