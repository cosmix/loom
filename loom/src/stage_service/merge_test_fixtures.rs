use std::path::{Path, PathBuf};

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("no-global-config"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub(crate) fn failing_signer_repo() -> (tempfile::TempDir, PathBuf, String) {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    std::fs::write(root.join("base"), "base").unwrap();
    git(root, &["add", "base"]);
    git(root, &["commit", "-m", "base"]);
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
