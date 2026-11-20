//! The listing around a provision runs on the host, in a worktree whose
//! `.git` file the stage controls: it is pinned. Its entries keep their columns.
//! Also: the busy line the daemon tick carries while an entry runs.

use super::*;
use crate::process::sandbox_probe::confined_commands_runnable;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};

/// A stage repointed its worktree's `.git` at a repository of its own whose
/// config names a clean filter: a host process that follows the pointer runs
/// it. The positive control runs last: git that follows the pointer does run
/// the filter, so its absence before comes from the pin.
#[test]
#[serial_test::serial]
fn the_listing_around_a_provision_does_not_follow_a_forged_git_file() {
    if !confined_commands_runnable() {
        return;
    }
    let _env = crate::test_cache_env::CacheEnv::pin();
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
#[serial_test::serial]
fn a_directory_no_repository_registers_skips_the_comparison() {
    if !confined_commands_runnable() {
        return;
    }
    let worktree = TempDir::new().unwrap();

    let (passed, stage) = gate(&["touch provisioned"], Vec::new(), worktree.path());

    assert!(passed);
    assert_eq!(stage.close_reason, None);
    assert!(worktree.path().join("provisioned").exists());
}

/// The listing's column and rename parsing is `parse_porcelain_v1_z`'s; the reason
/// names a rename by its destination only.
#[test]
fn a_renamed_file_is_reported_by_its_new_path() {
    let renamed = StatusEntry {
        code: *b"R ",
        path: "new.txt".into(),
        origin: Some("old.txt".into()),
    };
    let reason = unignored_reason(&[&renamed]);

    assert!(reason.contains("worktree: new.txt;"), "{reason}");
    assert!(!reason.contains("old.txt"), "{reason}");
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
#[serial_test::serial]
fn a_running_entry_sees_the_provisioning_busy_line() {
    if !confined_commands_runnable() {
        return;
    }
    let _env = crate::test_cache_env::CacheEnv::pin();
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
#[serial_test::serial]
fn each_entry_gets_its_own_busy_wrap() {
    if !confined_commands_runnable() {
        return;
    }
    let _env = crate::test_cache_env::CacheEnv::pin();
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
#[serial_test::serial]
fn a_failing_entry_stops_the_run_and_clears_the_busy_line() {
    let _env = crate::test_cache_env::CacheEnv::pin();
    let work = TempDir::new().unwrap();
    let worktree = TempDir::new().unwrap();
    let entries = [entry("exit 3"), entry("touch second")];

    let reason = provision_worktree(work.path(), "stage-a", &entries, worktree.path())
        .expect_err("the first entry fails");

    assert!(reason.contains("exit 3"), "{reason}");
    assert!(!worktree.path().join("second").exists());
    assert_eq!(tick::read(work.path()).unwrap().unwrap().busy, None);
}

/// A block that cannot persist (no stage file) is an error, not a quiet `Ok(false)`: the
/// scheduler records it, where a Queued stage would re-run its provision every tick.
#[test]
fn a_block_that_cannot_persist_is_an_error_naming_the_stage() {
    let temp = TempDir::new().unwrap();
    let work = prepare_work_dir(temp.path());
    let mut orchestrator = orchestrator_for(&work, temp.path());

    let error = orchestrator
        .block_for_provision("ghost", "provision failed".to_string())
        .expect_err("no stage file to block");

    assert!(format!("{error:#}").contains("ghost"), "{error:#}");
}

#[test]
fn a_persisted_block_answers_false() {
    let temp = TempDir::new().unwrap();
    let work = prepare_work_dir(temp.path());
    queued_stage(&work, Vec::new());
    let mut orchestrator = orchestrator_for(&work, temp.path());

    let spawned = orchestrator
        .block_for_provision("alpha", "provision failed".to_string())
        .unwrap();

    assert!(!spawned);
    assert_eq!(
        load_stage("alpha", &work).unwrap().status,
        StageStatus::Blocked
    );
}

#[test]
#[serial_test::serial]
fn every_provision_entry_is_given_the_stage_cache() {
    let env = crate::test_cache_env::CacheEnv::pin();
    let (repo, work) = crate::test_cache_env::repo_with_work_dir();
    let roots = std::cell::RefCell::new(Vec::new());

    provision_with(
        &work,
        "alpha",
        &[entry("true"), entry("true")],
        repo.path(),
        |_, _, cache| {
            roots
                .borrow_mut()
                .push(cache.map(|c| c.root().to_path_buf()));
            Ok(())
        },
    )
    .unwrap();

    let expected = Some(env.root_of(repo.path(), "alpha"));
    assert_eq!(*roots.borrow(), vec![expected.clone(), expected]);
}
