use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::save_stage;
use std::path::Path;
use std::process::{Command, Output};

fn run(root: &Path, program: &str, args: &[&str]) -> Output {
    let mut command = Command::new(program);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("no-global-config"))
        .output()
        .unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let output = run(root, "git", args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup(root: &Path) -> std::path::PathBuf {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    std::fs::write(root.join("seed"), "seed").unwrap();
    git(root, &["add", "seed"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    let worktree = root.join(".worktrees/skipped-stage");
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "loom/skipped-stage",
            worktree.to_str().unwrap(),
        ],
    );
    let work = root.join(".loom/work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let stage = Stage {
        id: "skipped-stage".into(),
        status: StageStatus::Skipped,
        cleanup_warning: Some("cleanup failed".into()),
        ..Stage::default()
    };
    save_stage(&stage, &work).unwrap();
    worktree
}

#[test]
fn skipped_cleanup_requires_explicit_confirmation_and_removes_resources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let worktree = setup(root);
    let binary = env!("CARGO_BIN_EXE_loom");
    let refused = run(
        root,
        binary,
        &["worktree", "remove", "skipped-stage", "--force"],
    );
    assert!(!refused.status.success());
    assert!(worktree.exists());
    let removed = run(
        root,
        binary,
        &[
            "worktree",
            "remove",
            "skipped-stage",
            "--force",
            "--confirm",
            "delete-unmerged-work:skipped-stage",
        ],
    );
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!worktree.exists());
}
