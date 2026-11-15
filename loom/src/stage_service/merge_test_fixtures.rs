use std::path::PathBuf;

use crate::test_git::{git, TestRepo};

pub(crate) fn failing_signer_repo() -> (tempfile::TempDir, PathBuf, String) {
    let repo = TestRepo::builder()
        .seed("base", "base", "base")
        .create()
        .into_temp_dir();
    let root = repo.path();
    git(root, &["checkout", "-b", "loom/s"]);
    std::fs::write(root.join("work"), "work").unwrap();
    git(root, &["add", "work"]);
    git(root, &["commit", "-m", "work"]);
    let commit = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "main"]);
    crate::git::signing::tests::fake_signer(root, true);
    let work = root.join(".loom/work");
    (repo, work, commit)
}
