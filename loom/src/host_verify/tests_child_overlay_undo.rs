//! The undo of a stopped trial's overlay through the runner's own entry,
//! `signals::kill_groups`, on a record with no process groups, and the
//! admission a trial's child takes before its first write. Neither needs a
//! confined child: the tests write the overlay bytes themselves.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use serial_test::serial;

use super::child_contracts::admit_overlay;
use super::signals::kill_groups;
use super::tests_child_contracts::{Fixture, STAGE};
use super::{store, JobRecord, JobState};
use crate::verify::contracts::test_support::{CONTRACT_CONTENT, CONTRACT_FILE};

const OVERLAY: &str = "PASS\n";
const MODE: u32 = 0o640;

/// A fixture whose contract file has mode [`MODE`].
fn fixture() -> Fixture {
    let fx = Fixture::new(&[("passes", "true")], &[]);
    let path = fx.worktree.join(CONTRACT_FILE);
    fs::set_permissions(&path, fs::Permissions::from_mode(MODE)).unwrap();
    fx
}

/// A trial of [`OVERLAY`] that was withdrawn after its child wrote it.
fn withdrawn_trial(fx: &Fixture) -> JobRecord {
    let mut record = fx.job(true, &[(CONTRACT_FILE, OVERLAY)]);
    record.state = JobState::Cancelled;
    store::save_current(&fx.work_dir, &record).unwrap();
    record
}

fn write_contract_file(fx: &Fixture, bytes: &str) {
    fs::write(fx.worktree.join(CONTRACT_FILE), bytes).unwrap();
}

#[test]
#[serial]
fn retiring_a_stopped_trial_puts_the_frozen_bytes_back() {
    let fx = fixture();
    let record = withdrawn_trial(&fx);
    write_contract_file(&fx, OVERLAY);
    kill_groups(&fx.work_dir, STAGE, &record.id);
    assert_eq!(fx.contract_file(), (CONTRACT_CONTENT.to_vec(), MODE));
}

#[test]
#[serial]
fn retiring_a_trial_leaves_a_file_without_its_bytes_alone() {
    let fx = fixture();
    let record = withdrawn_trial(&fx);
    write_contract_file(&fx, "edited after the trial\n");
    kill_groups(&fx.work_dir, STAGE, &record.id);
    assert_eq!(
        fx.contract_file(),
        (b"edited after the trial\n".to_vec(), MODE)
    );

    let plain = fx.job(true, &[]);
    write_contract_file(&fx, OVERLAY);
    kill_groups(&fx.work_dir, STAGE, &plain.id);
    assert_eq!(
        fx.contract_file(),
        (OVERLAY.as_bytes().to_vec(), MODE),
        "a job without replacements undoes nothing"
    );
}

#[test]
#[serial]
fn retiring_a_stale_trial_keeps_a_newer_running_trials_overlay() {
    let fx = fixture();
    let stale = withdrawn_trial(&fx);
    let _newer = fx.job(true, &[(CONTRACT_FILE, OVERLAY)]);
    write_contract_file(&fx, OVERLAY);
    kill_groups(&fx.work_dir, STAGE, &stale.id);
    assert_eq!(fx.contract_file(), (OVERLAY.as_bytes().to_vec(), MODE));

    // A newer running job that overlays nothing does not keep the stale one's.
    let _check = fx.job(true, &[]);
    kill_groups(&fx.work_dir, STAGE, &stale.id);
    assert_eq!(fx.contract_file(), (CONTRACT_CONTENT.to_vec(), MODE));
}

#[test]
#[serial]
fn only_the_running_job_is_admitted_to_write_its_overlay() {
    let fx = fixture();
    let mut record = fx.job(true, &[(CONTRACT_FILE, OVERLAY)]);
    drop(admit_overlay(&fx.work_dir, &record).expect("the running job is admitted"));

    record.state = JobState::Interrupted;
    store::save_current(&fx.work_dir, &record).unwrap();
    let Err(error) = admit_overlay(&fx.work_dir, &record) else {
        panic!("an interrupted job must not be admitted");
    };
    assert!(
        format!("{error:#}").contains("no longer running"),
        "{error:#}"
    );

    let _newer = fx.job(true, &[]);
    assert!(
        admit_overlay(&fx.work_dir, &record).is_err(),
        "a superseded job is refused"
    );
}
