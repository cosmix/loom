//! Contract: the DONE rename resolves a relative plan path against the
//! project root, whatever the process working directory is.

use crate::fs::plan_lifecycle::mark_plan_done_if_all_merged;
use crate::fs::work_dir::WorkDir;
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

#[test]
#[serial]
fn done_rename_finds_a_relative_plan_from_any_working_directory() {
    let root = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let work_dir = WorkDir::new(root.path()).unwrap();
    work_dir.initialize().unwrap();
    let plans = root.path().join("doc/plans");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(plans.join("IN_PROGRESS-PLAN-x.md"), "# Plan").unwrap();
    std::fs::write(
        work_dir.root().join("config.toml"),
        "[plan]\nsource_path = \"doc/plans/IN_PROGRESS-PLAN-x.md\"\nplan_id = \"test\"\nplan_name = \"Test\"\nbase_branch = \"main\"\n",
    )
    .unwrap();
    let stages = work_dir.root().join("stages");
    std::fs::create_dir_all(&stages).unwrap();
    std::fs::write(
        stages.join("0-s1.md"),
        "---\nid: s1\nname: S\nstatus: Completed\nmerged: true\n---\n# Stage\n",
    )
    .unwrap();
    let cwd = CwdGuard::enter(elsewhere.path());

    let result = mark_plan_done_if_all_merged(&work_dir);
    drop(cwd);

    let renamed = result
        .expect("the DONE rename must not fail")
        .expect("a fully merged plan must be renamed");
    let name = renamed.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("DONE-"), "{name}");
    assert!(plans.join("DONE-PLAN-x.md").is_file());
    assert!(!plans.join("IN_PROGRESS-PLAN-x.md").exists());
}
