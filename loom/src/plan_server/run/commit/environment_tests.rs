//! A commit's git steps name the server's repository, whatever repository the
//! shell that started the server exported.

use std::path::Path;
use std::process::Command;

use super::{strings, Repo};
use crate::plan_server::wire::run::ChangedPath;

const ROOT_ENV: &str = "LOOM_TEST_COMMIT_ROOT";

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
