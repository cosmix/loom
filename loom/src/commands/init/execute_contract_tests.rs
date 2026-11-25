//! Contracts: `loom init --clean <plan>` refuses a plan it cannot run before
//! it deletes the previous run's state directory and worktrees.

use super::execute;
use crate::fs::permissions::scratch_home::ScratchHome;
use crate::git::target_guard::test_support::{git, repo, Repo};
use crate::plan::schema::{LoomConfig, LoomMetadata, StageDefinition, WiringCheck};
use serial_test::serial;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Enters `dir` and returns to the previous directory on drop.
struct CwdGuard(PathBuf);

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self(original)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

fn stage(id: &str, dependencies: &[&str]) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: format!("Stage {id}"),
        working_dir: ".".to_string(),
        dependencies: dependencies.iter().map(|d| d.to_string()).collect(),
        artifacts: vec!["README.md".to_string()],
        wiring: vec![WiringCheck {
            source: "README.md".to_string(),
            pattern: "readme".to_string(),
            description: "the readme is present".to_string(),
            literal: false,
        }],
        ..Default::default()
    }
}

/// A plan file holding `stages`, written into `dir` (outside the repository).
fn write_plan(dir: &Path, stages: Vec<StageDefinition>) -> PathBuf {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages,
            ..Default::default()
        },
    };
    let yaml = crate::yaml::to_string(&metadata).unwrap();
    let text = format!(
        "# Contract Plan\n\n<!-- loom METADATA -->\n```yaml\n{yaml}```\n<!-- END loom METADATA -->\n"
    );
    let path = dir.join("PLAN-contract.md");
    std::fs::write(&path, text).unwrap();
    path
}

/// The previous run: a state config, a state file and a stage worktree file.
fn previous_run(repo: &Repo) -> (PathBuf, PathBuf) {
    std::fs::write(repo.work.join("config.toml"), "[plan]\nplan_id = \"old\"\n").unwrap();
    let state_file = repo.work.join("keep.txt");
    std::fs::write(&state_file, "state").unwrap();
    let worktree_file = repo.root.join(".worktrees/stage-a/keep.txt");
    std::fs::create_dir_all(worktree_file.parent().unwrap()).unwrap();
    std::fs::write(&worktree_file, "work").unwrap();
    (state_file, worktree_file)
}

fn assert_previous_run_intact(state_file: &Path, worktree_file: &Path) {
    assert_eq!(
        std::fs::read_to_string(state_file).ok().as_deref(),
        Some("state"),
        "the previous run's state file was deleted"
    );
    assert_eq!(
        std::fs::read_to_string(worktree_file).ok().as_deref(),
        Some("work"),
        "the previous run's worktree directory was deleted"
    );
}

#[test]
#[serial]
fn cyclic_plan_leaves_the_previous_run_untouched() {
    let _home = ScratchHome::set();
    let repo = repo();
    let (state_file, worktree_file) = previous_run(&repo);
    let plan_dir = TempDir::new().unwrap();
    let plan = write_plan(
        plan_dir.path(),
        vec![stage("a", &["b"]), stage("b", &["a"])],
    );
    let _cwd = CwdGuard::enter(&repo.root);

    let result = execute(Some(plan), true, true);

    let error = format!("{:#}", result.expect_err("a cyclic plan must be refused"));
    assert!(error.contains("Circular dependency detected"), "{error}");
    assert_previous_run_intact(&state_file, &worktree_file);
}

#[test]
#[serial]
fn detached_head_leaves_the_previous_run_untouched() {
    let _home = ScratchHome::set();
    let repo = repo();
    let (state_file, worktree_file) = previous_run(&repo);
    let plan_dir = TempDir::new().unwrap();
    let plan = write_plan(plan_dir.path(), vec![stage("a", &[])]);
    git(&repo.root, &["checkout", "-q", "--detach"]);
    let _cwd = CwdGuard::enter(&repo.root);

    let result = execute(Some(plan), true, true);

    let error = format!("{:#}", result.expect_err("a detached HEAD must be refused"));
    assert!(error.contains("HEAD is detached"), "{error}");
    assert_previous_run_intact(&state_file, &worktree_file);
}
