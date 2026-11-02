//! `loom stage merge <id> --resolved` through the real binary, run as an
//! operator from inside the stage worktree: no loom session environment, so
//! the command takes the local path rather than relaying to a daemon.
//!
//! Only synchronous `git` commands and the synchronous binary run here: no
//! daemon, no terminal, no session.

#[path = "test_support/mod.rs"]
mod test_support;

use loom::models::stage::{Stage, StageStatus};
use loom::verify::transitions::{load_stage, save_stage};
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// Run git with the user's and system's config out of the picture.
fn run_git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = run_git(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_file(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", name]);
}

/// A loom project whose stage `s` is in `MergeConflict`: `main` and the stage
/// worktree edited `seed.txt` differently, and `completed_commit` records the
/// stage's own commit.
struct Project {
    _temp: TempDir,
    root: PathBuf,
    worktree: PathBuf,
    work_dir: PathBuf,
}

impl Project {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.email", "t@t.com"]);
        git(&root, &["config", "user.name", "t"]);
        std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
        commit_file(&root, "seed.txt", "seed");
        let worktree = root.join(".worktrees").join("s");
        let path = worktree.to_str().unwrap();
        git(&root, &["worktree", "add", "-q", "-b", "loom/s", path]);
        commit_file(&worktree, "seed.txt", "stage side");
        let own_commit = git(&worktree, &["rev-parse", "HEAD"]);
        commit_file(&root, "seed.txt", "main side");

        // The state directory lives in the main checkout; the worktree reaches
        // it through the `.loom/work` symlink every stage worktree has.
        let work_dir = root.join(".loom").join("work");
        test_support::write_main_checkout_identity(&root, &work_dir);
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();
        std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
        std::fs::create_dir_all(worktree.join(".loom")).unwrap();
        std::os::unix::fs::symlink("../../../.loom/work", worktree.join(".loom/work")).unwrap();
        let mut stage = Stage::new("Merge CLI".to_string(), None);
        stage.id = "s".to_string();
        stage.status = StageStatus::MergeConflict;
        stage.completed_commit = Some(own_commit);
        stage.worktree = Some("s".to_string());
        stage.base_branch = Some("main".to_string());
        stage.resolved_base = Some("main".to_string());
        stage.working_dir = Some(".".to_string());
        save_stage(&stage, &work_dir).unwrap();
        Self {
            _temp: temp,
            root,
            worktree,
            work_dir,
        }
    }

    /// `loom stage merge s --resolved` from inside the worktree, with no
    /// `LOOM_*` variable in its environment.
    fn merge_resolved(&self) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LOOM_") {
                command.env_remove(key);
            }
        }
        command
            .args(["stage", "merge", "s", "--resolved"])
            .current_dir(&self.worktree)
            .output()
            .expect("the loom binary must start and exit")
    }

    fn stage(&self) -> Stage {
        load_stage("s", &self.work_dir).unwrap()
    }
}

#[test]
#[serial]
fn resolved_from_inside_the_worktree_lands_the_merge_and_defers_the_cleanup() {
    let project = Project::new();
    let wt = &project.worktree;
    assert!(!run_git(wt, &["merge", "main"]).status.success());
    std::fs::write(wt.join("seed.txt"), "resolved").unwrap();
    git(wt, &["add", "seed.txt"]);
    git(wt, &["commit", "-q", "-m", "resolve"]);

    let output = project.merge_resolved();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(git(&project.root, &["show", "main:seed.txt"]), "resolved");
    let branch_head = git(wt, &["rev-parse", "HEAD"]);
    assert!(run_git(
        &project.root,
        &["merge-base", "--is-ancestor", &branch_head, "main"]
    )
    .status
    .success());
    assert_eq!(
        git(&project.root, &["rev-parse", "HEAD"]),
        git(&project.root, &["rev-parse", "main"])
    );
    let stage = project.stage();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert!(wt.is_dir(), "the command ran inside it, so it stays");
    assert!(
        stdout.contains("Worktree cleanup deferred"),
        "stdout says why: {stdout}"
    );
}

#[test]
#[serial]
fn resolved_with_the_merge_still_in_progress_is_refused_and_changes_nothing() {
    let project = Project::new();
    let wt = &project.worktree;
    assert!(!run_git(wt, &["merge", "main"]).status.success());
    let main_before = git(&project.root, &["rev-parse", "main"]);

    let output = project.merge_resolved();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr: {stderr}");
    assert!(stderr.contains("not ready to merge"), "stderr: {stderr}");
    assert!(
        stderr.contains("merge"),
        "stderr names the reason: {stderr}"
    );
    let stage = project.stage();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(!stage.merged);
    assert_eq!(git(&project.root, &["rev-parse", "main"]), main_before);
    assert!(wt.is_dir());
}
