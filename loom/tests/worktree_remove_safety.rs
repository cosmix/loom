use loom::commands::worktree_cmd;
use loom::models::stage::{Stage, StageStatus, StageType};
use loom::verify::transitions::{load_stage, save_stage, update_stage};
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
        .env("GIT_CONFIG_SYSTEM", root.join(".no-system-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn git_path(root: &Path, name: &str) -> PathBuf {
    let path = PathBuf::from(git_stdout(root, &["rev-parse", "--git-path", name]));
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "test@example.com"]);
    git_ok(root, &["config", "user.name", "Test"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git_ok(root, &["add", "README.md"]);
    git_ok(root, &["commit", "-m", "base"]);
    fs::create_dir_all(root.join(".work/stages")).unwrap();
    fs::write(
        root.join(".work/config.toml"),
        "[plan]\nbase_branch = \"main\"\n",
    )
    .unwrap();
    temp
}

fn create_stage(root: &Path, stage_id: &str) -> (PathBuf, String) {
    let worktrees = root.join(".worktrees");
    fs::create_dir_all(&worktrees).unwrap();
    let worktree = worktrees.join(stage_id);
    let path = worktree.to_string_lossy().to_string();
    let branch = format!("loom/{stage_id}");
    git_ok(root, &["worktree", "add", "-b", &branch, &path, "main"]);
    fs::write(worktree.join("feature.txt"), "feature\n").unwrap();
    git_ok(&worktree, &["add", "feature.txt"]);
    git_ok(&worktree, &["commit", "-m", "feature"]);
    let commit = git_stdout(&worktree, &["rev-parse", "HEAD"]);

    let mut stage = Stage::new(stage_id.to_string(), Some("test stage".to_string()));
    stage.id = stage_id.to_string();
    stage.stage_type = StageType::Standard;
    stage.status = StageStatus::Completed;
    stage.completed_at = Some(chrono::Utc::now());
    stage.completed_commit = Some(commit.clone());
    stage.merged = false;
    save_stage(&stage, &root.join(".work")).unwrap();
    (worktree, commit)
}

fn write_completed_stage(root: &Path, stage_id: &str, commit: String) {
    let mut stage = Stage::new(stage_id.to_string(), Some("test stage".to_string()));
    stage.id = stage_id.to_string();
    stage.stage_type = StageType::Standard;
    stage.status = StageStatus::Completed;
    stage.completed_at = Some(chrono::Utc::now());
    stage.completed_commit = Some(commit);
    save_stage(&stage, &root.join(".work")).unwrap();
}

fn merge_stage(root: &Path, stage_id: &str) {
    let branch = format!("loom/{stage_id}");
    git_ok(root, &["merge", "--no-ff", "-m", "merge stage", &branch]);
}

fn in_repo<T>(root: &Path, operation: impl FnOnce() -> T) -> T {
    let prior = std::env::current_dir().unwrap();
    std::env::set_current_dir(root).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
    std::env::set_current_dir(prior).unwrap();
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
#[serial]
fn normal_remove_preserves_dirty_uncommitted_work() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage(root, "dirty");
    merge_stage(root, "dirty");
    fs::write(worktree.join("feature.txt"), "keep this edit\n").unwrap();

    let error = in_repo(root, || worktree_cmd::remove("dirty".into(), false, None)).unwrap_err();
    assert!(error.to_string().contains("uncommitted changes"));
    assert!(worktree.join("feature.txt").exists());
    assert!(!load_stage("dirty", &root.join(".work")).unwrap().merged);
}

#[test]
#[serial]
fn normal_remove_preserves_clean_but_unmerged_commit() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage(root, "unmerged");

    let error = in_repo(root, || {
        worktree_cmd::remove("unmerged".into(), false, None)
    })
    .unwrap_err();
    assert!(error.to_string().contains("not retained"));
    assert!(worktree.exists());
    assert!(!load_stage("unmerged", &root.join(".work")).unwrap().merged);
}

#[test]
#[serial]
fn normal_remove_preserves_commit_added_after_recorded_completion() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage(root, "advanced");
    merge_stage(root, "advanced");
    fs::write(worktree.join("later.txt"), "later commit\n").unwrap();
    git_ok(&worktree, &["add", "later.txt"]);
    git_ok(&worktree, &["commit", "-m", "later work"]);

    let error = in_repo(root, || {
        worktree_cmd::remove("advanced".into(), false, None)
    })
    .unwrap_err();

    assert!(error.to_string().contains("stage branch head"));
    assert!(worktree.join("later.txt").exists());
    assert!(!load_stage("advanced", &root.join(".work")).unwrap().merged);
}

#[test]
#[serial]
fn normal_remove_requires_a_retained_completed_commit() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage(root, "missing-commit");
    let work_dir = root.join(".work");
    update_stage("missing-commit", &work_dir, |stage| {
        stage.completed_commit = None;
        Ok(())
    })
    .unwrap();

    let error = in_repo(root, || {
        worktree_cmd::remove("missing-commit".into(), false, None)
    })
    .unwrap_err();
    assert!(error.to_string().contains("no retained completed commit"));
    assert!(worktree.exists());
    assert!(!load_stage("missing-commit", &work_dir).unwrap().merged);
}

#[test]
#[serial]
fn absent_resources_never_create_a_phantom_merge() {
    let temp = init_repo();
    let root = temp.path();
    let commit = git_stdout(root, &["rev-parse", "main"]);
    write_completed_stage(root, "absent", commit);

    let error = in_repo(root, || worktree_cmd::remove("absent".into(), false, None)).unwrap_err();
    assert!(error.to_string().contains("refusing to infer"));
    assert!(!load_stage("absent", &root.join(".work")).unwrap().merged);
}

#[test]
#[serial]
fn legitimately_merged_cleanup_marks_stage_only_after_removal() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage(root, "merged");
    merge_stage(root, "merged");

    in_repo(root, || worktree_cmd::remove("merged".into(), false, None)).unwrap();
    assert!(!worktree.exists());
    assert!(load_stage("merged", &root.join(".work")).unwrap().merged);
    assert!(
        !git(root, &["show-ref", "--verify", "refs/heads/loom/merged"])
            .status
            .success()
    );
}

#[test]
#[serial]
fn confirmed_destructive_remove_never_fabricates_merge_state() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage(root, "forced");
    let confirmation = "delete-unmerged-work:forced".to_string();

    in_repo(root, || {
        worktree_cmd::remove("forced".into(), true, Some(confirmation))
    })
    .unwrap();

    assert!(!worktree.exists());
    assert!(!load_stage("forced", &root.join(".work")).unwrap().merged);
}

#[test]
#[serial]
fn cleanup_error_is_fatal_and_does_not_mark_stage_merged() {
    let temp = init_repo();
    let root = temp.path();
    let (worktree, _) = create_stage(root, "cleanup-error");
    merge_stage(root, "cleanup-error");
    fs::write(git_path(&worktree, "info/exclude"), ".work/\n").unwrap();
    fs::create_dir(worktree.join(".work")).unwrap();
    fs::write(worktree.join(".work/user-data.txt"), "preserve me\n").unwrap();
    assert_eq!(
        git_stdout(
            &worktree,
            &[
                "status",
                "--porcelain=v1",
                "--untracked-files=all",
                "--ignored=matching",
            ],
        ),
        "!! .work/"
    );

    let result = in_repo(root, || {
        worktree_cmd::remove("cleanup-error".into(), false, None)
    });

    assert!(result.is_err(), "cleanup failure must surface");
    assert!(format!("{:#}", result.unwrap_err()).contains("non-symlink worktree scaffold"));
    assert!(worktree.exists());
    assert!(worktree.join(".work/user-data.txt").exists());
    assert!(
        !load_stage("cleanup-error", &root.join(".work"))
            .unwrap()
            .merged
    );
}
