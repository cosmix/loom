use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::save_stage;
use std::path::Path;
use std::process::{Command, Output};

#[path = "test_support/mod.rs"]
mod test_support;

use test_support::git_repo::{git, isolate, TestRepo};

fn run(root: &Path, program: &str, args: &[&str]) -> Output {
    let mut command = Command::new(program);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    isolate(&mut command);
    command.args(args).current_dir(root).output().unwrap()
}

fn setup(root: &Path) -> std::path::PathBuf {
    TestRepo::builder()
        .identity("Test", "test@example.com")
        .seed("seed", "seed", "seed")
        .init_in(root);
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
