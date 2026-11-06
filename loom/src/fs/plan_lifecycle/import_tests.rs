//! Finalization records the run review's open findings in the feature ledger.

use super::*;
use crate::fs::memory::{append_entry, MemoryEntry, MemoryEntryType};
use tempfile::TempDir;

fn finalize_fixture(temp: &TempDir, plan_rel: &str) -> WorkDir {
    let work = WorkDir::new(temp.path()).unwrap();
    work.initialize().unwrap();
    let plan = temp.path().join(plan_rel);
    fs::create_dir_all(plan.parent().unwrap()).unwrap();
    fs::write(
        &plan,
        "# Plan\n\nImplements: section_02#budget, section_03\n",
    )
    .unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"PLAN-feature\"\nplan_name = \"Feature\"\n\
         base_branch = \"main\"\n",
        plan.display()
    );
    fs::write(work.root().join("config.toml"), config).unwrap();
    let stages = work.root().join("stages");
    fs::create_dir_all(&stages).unwrap();
    fs::write(
        stages.join("0-build.md"),
        "---\nid: build\nname: Build\nstatus: Completed\nmerged: true\n---\n# Stage\n",
    )
    .unwrap();
    let entry = MemoryEntry::new(MemoryEntryType::Suggestion, "Rename the flag".into());
    append_entry(work.root(), "build", &entry).unwrap();
    work
}

fn ledger(temp: &TempDir) -> String {
    fs::read_to_string(temp.path().join("doc/plans/demo/findings.md")).unwrap()
}

#[test]
fn completion_writes_the_ledger_once() {
    let temp = TempDir::new().unwrap();
    let work = finalize_fixture(&temp, "doc/plans/demo/IN_PROGRESS-PLAN-feature.md");
    let done = mark_plan_done_if_all_merged(&work).unwrap().unwrap();
    assert!(done.exists());
    let first = ledger(&temp);
    assert_eq!(first.matches("\n## F-").count(), 1, "ledger:\n{first}");
    assert!(first.contains("About: PLAN-feature.md#stage:build, section_02#budget, section_03\n"));
    assert!(first.contains("Source: REVIEW-PLAN-feature.md#open-findings (build, "));

    assert!(mark_plan_done_if_all_merged(&work).unwrap().is_none());
    assert_eq!(ledger(&temp), first);
}

#[test]
fn a_standalone_plan_writes_no_ledger() {
    let temp = TempDir::new().unwrap();
    let work = finalize_fixture(&temp, "doc/plans/IN_PROGRESS-PLAN-feature.md");
    mark_plan_done_if_all_merged(&work).unwrap().unwrap();
    assert!(!temp.path().join("doc/plans/findings.md").exists());
}

#[test]
fn a_symlinked_ledger_keeps_the_plan_retryable() {
    let temp = TempDir::new().unwrap();
    let work = finalize_fixture(&temp, "doc/plans/demo/IN_PROGRESS-PLAN-feature.md");
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("findings.md");
    fs::write(&target, "keep\n").unwrap();
    std::os::unix::fs::symlink(&target, temp.path().join("doc/plans/demo/findings.md")).unwrap();

    assert!(mark_plan_done_if_all_merged(&work).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");
    let plan = temp
        .path()
        .join("doc/plans/demo/IN_PROGRESS-PLAN-feature.md");
    assert!(plan.exists());
}
