//! What each caller of the resolver's completion checks gets when a detector
//! cannot run.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::*;
use crate::context::graph_store::GraphStore;
use crate::context::store::CACHE_RELATIVE_DIR;
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::save_stage;

const STAGE: &str = "s1";

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A stage worktree in merge resolution with committed work. A regular file
/// sits where the source graph's base layers are listed: the merge and
/// knowledge checks pass, and both detectors fail building their graph.
fn resolved_stage_with_broken_detector(root: &Path) -> (Stage, PathBuf) {
    let work_dir = root.join(".loom/work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    crate::git::worktree::capture_main_identity(root, &work_dir).unwrap();
    git(root, &["config", "user.name", "loom-test"]);
    git(root, &["config", "user.email", "loom-test@example.com"]);
    std::fs::write(root.join(".gitignore"), ".loom/\n.worktrees/\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    let worktree = root.join(".worktrees").join(STAGE);
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "loom/s1",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("b.txt"), "b\n").unwrap();
    git(&worktree, &["add", "b.txt"]);
    git(&worktree, &["commit", "-q", "-m", "stage work"]);
    let base_dir = GraphStore::new(&root.join(CACHE_RELATIVE_DIR), &work_dir).base_dir();
    std::fs::create_dir_all(base_dir.parent().unwrap()).unwrap();
    std::fs::write(&base_dir, "not a directory").unwrap();
    let stage = Stage {
        id: STAGE.to_owned(),
        name: STAGE.to_owned(),
        status: StageStatus::MergeConflict,
        completed_commit: Some(git(&worktree, &["rev-parse", "HEAD"])),
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    (stage, work_dir)
}

#[test]
fn the_daemon_refuses_a_landing_its_detectors_could_not_check() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let (stage, work_dir) = resolved_stage_with_broken_detector(&root);

    let error = verify_resolved_completion(&stage, &work_dir, |_, _| {}).unwrap_err();

    assert!(
        format!("{error:#}").contains("Wiring detection could not run"),
        "{error:#}"
    );
}

#[test]
fn failed_acceptance_names_only_the_failing_commands() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let (mut stage, work_dir) = resolved_stage_with_broken_detector(&root);
    stage.acceptance = ["exit 0", "exit 3", "exit 4"]
        .map(|command| AcceptanceCriterion::Simple(command.to_owned()))
        .to_vec();

    let error = verify_resolved_completion(&stage, &work_dir, |_, _| {}).unwrap_err();

    assert!(
        format!("{error:#}").ends_with("failed for stage 's1': exit 3; exit 4"),
        "{error:#}"
    );
}

#[test]
fn the_failing_command_list_is_bounded() {
    let failing = |command: String| {
        CriterionResult::new(
            command,
            false,
            String::new(),
            String::new(),
            Some(1),
            std::time::Duration::ZERO,
            false,
        )
    };
    let results: Vec<_> = (0..40)
        .map(|index| failing(format!("run-{index} {}", "x".repeat(500))))
        .collect();

    let listed = failed_commands(&results);

    assert_eq!(listed.chars().count(), 1024);
    assert!(listed.starts_with("run-0 "), "{listed}");
    assert!(listed.ends_with('…'), "{listed}");
    assert!(listed.split("; ").all(|one| one.chars().count() <= 200));
}

#[test]
fn an_operator_is_warned_and_keeps_the_decision() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let (stage, work_dir) = resolved_stage_with_broken_detector(&root);

    verify_resolved_completion_for_operator(&stage, &work_dir, |_, _| {}).unwrap();
}
