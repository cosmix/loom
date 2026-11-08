//! The listing around a provision runs on the host, in a worktree whose
//! `.git` file the stage controls: it is pinned. Its entries keep their columns.
//! Also: the busy line the daemon tick carries while an entry runs.

use super::*;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};

/// A stage repointed its worktree's `.git` at a repository of its own whose
/// config names a clean filter: a host process that follows the pointer runs
/// it. The positive control runs last: git that follows the pointer does run
/// the filter, so its absence before comes from the pin.
#[test]
fn the_listing_around_a_provision_does_not_follow_a_forged_git_file() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().canonicalize().unwrap().join("repo");
    let worktree = contract_worktree(&repo, "alpha");
    let marker = plant_foreign_git_dir(&worktree);
    let work = prepare_work_dir(&repo);

    provision_worktree(&work, "alpha", &[entry("true")], &worktree).unwrap();

    assert!(
        !marker.exists(),
        "the provision listing ran the stage's filter"
    );
    crate::git::worktree::WorktreeGit::pinned_in_project_of(&work, &worktree)
        .unwrap()
        .run_checked(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .unwrap();
    assert!(
        status_entries(&work, &worktree).is_some(),
        "the listing ran"
    );
    assert!(
        !marker.exists(),
        "the provision listing ran the stage's filter"
    );
    crate::git::worktree::WorktreeGit::discovered(&worktree)
        .run(&["status", "--porcelain"])
        .unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}

/// A worktree that cannot be pinned is the unreadable-listing path: no
/// comparison, and no git in it either.
#[test]
fn a_directory_no_repository_registers_skips_the_comparison() {
    let worktree = TempDir::new().unwrap();

    let (passed, stage) = gate(&["touch provisioned"], Vec::new(), worktree.path());

    assert!(passed);
    assert_eq!(stage.close_reason, None);
    assert!(worktree.path().join("provisioned").exists());
}

#[test]
fn status_entries_keep_their_leading_space_and_rename_origins() {
    let listing = " M web/bun.lock\0R  new.txt\0old.txt\0?? added\0";

    assert_eq!(
        split_status_entries(listing),
        [" M web/bun.lock", "R  new.txt\0old.txt", "?? added"]
    );
}

#[test]
fn a_rename_in_the_worktree_column_keeps_its_origin_in_one_entry() {
    assert_eq!(
        split_status_entries(" R new.txt\0old.txt\0"),
        [" R new.txt\0old.txt"]
    );
}

#[test]
fn a_renamed_file_is_reported_by_its_new_path() {
    assert_eq!(
        unignored_reason(&["R  new.txt\0old.txt"]),
        unignored_reason(&["R  new.txt"])
    );
}

/// A work directory inside `worktree`: a provision entry runs sandboxed and sees only the
/// worktree, so a tick it copies must live there.
fn work_dir_in(worktree: &Path) -> std::path::PathBuf {
    let work = worktree.join(".work");
    fs::create_dir(&work).unwrap();
    work
}

/// A shell command that copies the daemon tick into the entry's worktree as `name`.
fn copy_tick_command(work: &Path, name: &str) -> String {
    format!("cp {} {name}", work.join("orchestrator.tick").display())
}

/// Read the tick an entry copied into `worktree` as `name`.
fn copied_tick(worktree: &Path, name: &str) -> tick::Tick {
    let dir = TempDir::new().unwrap();
    fs::copy(worktree.join(name), dir.path().join("orchestrator.tick")).unwrap();
    tick::read(dir.path())
        .unwrap()
        .expect("the copied tick reads")
}

fn running_provisioning(tick: &tick::Tick) -> &crate::orchestrator::busy::Busy {
    let busy = tick
        .busy
        .as_ref()
        .expect("the tick names the running entry");
    assert_eq!(busy.op, BusyOp::Provisioning);
    assert_eq!(busy.stage_id, "stage-a");
    busy
}

#[test]
fn a_running_entry_sees_the_provisioning_busy_line() {
    let worktree = TempDir::new().unwrap();
    let work = work_dir_in(worktree.path());
    let copy = entry(&copy_tick_command(&work, "seen.tick"));

    provision_worktree(&work, "stage-a", &[copy], worktree.path()).unwrap();

    let seen = copied_tick(worktree.path(), "seen.tick");
    assert_eq!(seen.phase, Some(Phase::Spawning));
    running_provisioning(&seen);
    let after = tick::read(&work).unwrap().unwrap();
    assert_eq!(after.busy, None, "the busy line ends with the entry");
}

#[test]
fn each_entry_gets_its_own_busy_wrap() {
    let worktree = TempDir::new().unwrap();
    let work = work_dir_in(worktree.path());
    let first = format!("{} && sleep 1", copy_tick_command(&work, "seen-1.tick"));
    let entries = [
        entry(&first),
        entry(&copy_tick_command(&work, "seen-2.tick")),
    ];

    provision_worktree(&work, "stage-a", &entries, worktree.path()).unwrap();

    let first = copied_tick(worktree.path(), "seen-1.tick");
    let second = copied_tick(worktree.path(), "seen-2.tick");
    assert!(
        running_provisioning(&second).started_at > running_provisioning(&first).started_at,
        "the second entry starts its own wrap"
    );
}

#[test]
fn a_failing_entry_stops_the_run_and_clears_the_busy_line() {
    let work = TempDir::new().unwrap();
    let worktree = TempDir::new().unwrap();
    let entries = [entry("exit 3"), entry("touch second")];

    let reason = provision_worktree(work.path(), "stage-a", &entries, worktree.path())
        .expect_err("the first entry fails");

    assert!(reason.contains("exit 3"), "{reason}");
    assert!(!worktree.path().join("second").exists());
    assert_eq!(tick::read(work.path()).unwrap().unwrap().busy, None);
}
