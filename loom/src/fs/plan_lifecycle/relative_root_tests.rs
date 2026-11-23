//! A relative `source_path` is relative to the plan's project root, never the working directory.

use super::*;
use serial_test::serial;
use tempfile::TempDir;

struct CwdGuard(PathBuf);

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self(previous)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[test]
#[serial]
fn done_rename_uses_the_project_root_not_the_working_directory() {
    let temp = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let work = WorkDir::new(temp.path()).unwrap();
    work.initialize().unwrap();
    let plans = temp.path().join("doc/plans");
    fs::create_dir_all(&plans).unwrap();
    fs::write(plans.join("IN_PROGRESS-PLAN-x.md"), "# Plan\n").unwrap();
    fs::write(
        work.root().join("config.toml"),
        "[plan]\nsource_path = \"doc/plans/IN_PROGRESS-PLAN-x.md\"\nplan_id = \"PLAN-x\"\n\
         plan_name = \"X\"\nbase_branch = \"main\"\n",
    )
    .unwrap();
    let stages = work.root().join("stages");
    fs::create_dir_all(&stages).unwrap();
    fs::write(
        stages.join("0-build.md"),
        "---\nid: build\nname: Build\nstatus: Completed\nmerged: true\n---\n# Stage\n",
    )
    .unwrap();

    let _cwd = CwdGuard::enter(elsewhere.path());
    let done = mark_plan_done_if_all_merged(&work).unwrap();

    assert_eq!(done, Some(PathBuf::from("doc/plans/DONE-PLAN-x.md")));
    assert!(plans.join("DONE-PLAN-x.md").exists());
    assert!(!plans.join("IN_PROGRESS-PLAN-x.md").exists());
}
