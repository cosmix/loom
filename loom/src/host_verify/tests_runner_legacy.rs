//! Legacy records preserve unresolved ownership evidence without signalling PIDs.

use std::fs::{self, File, FileTimes};
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::runner::recover_after_restart;
use super::store;
use super::tests_runner::{guarded_criterion, state_root, Criterion, STAGE};
use super::tests_runner_recovery::running_record;
use super::{JobRecord, JobState};

fn record_legacy_group(work: &Path, record: &JobRecord, pgid: u32) {
    let directory = store::job_dir(work, STAGE, &record.id).unwrap();
    fs::write(directory.join("groups"), format!("{pgid}\n")).unwrap();
}

fn record_reused_legacy_group(work: &Path, record: &JobRecord, pgid: u32) {
    // A stale guardian cannot claim another process generation's legacy line.
    let groups = store::job_dir(work, STAGE, &record.id)
        .unwrap()
        .join("groups");
    let boot = crate::process::boot_id::os_boot_id().unwrap();
    fs::write(groups, format!("{pgid} 0 {boot} guard-ab ab\n{pgid}\n")).unwrap();
}

fn report(work: &Path, record: &JobRecord) -> Vec<u8> {
    store::load_job_file(work, STAGE, &record.id, "retirement-pending.json")
        .unwrap()
        .expect("legacy retirement must leave durable evidence")
}

#[test]
fn recovery_preserves_legacy_evidence_and_still_retires_guarded_commands() {
    let (_directory, work) = state_root();
    let record = running_record(STAGE, None);
    store::save_current(&work, &record).unwrap();
    let mut stranger = Criterion::spawn();
    record_legacy_group(&work, &record, stranger.0.id());
    let (_anchor, mut criterion) = guarded_criterion(&work, &record);

    assert_eq!(recover_after_restart(&work).unwrap().len(), 1);
    assert!(criterion.killed());
    assert!(stranger.0.try_wait().unwrap().is_none());
    let first = report(&work, &record);
    let value: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(
        value["unverified_process_groups"],
        serde_json::json!([stranger.0.id()])
    );
    assert!(value["action"]
        .as_str()
        .unwrap()
        .contains("PGIDs alone are unsafe"));

    assert!(recover_after_restart(&work).unwrap().is_empty());
    assert_eq!(report(&work, &record), first);
    assert!(stranger.0.try_wait().unwrap().is_none());
}

#[test]
fn historical_legacy_jobs_keep_their_original_evidence_through_pruning() {
    let (_directory, work) = state_root();
    let mut old = running_record(STAGE, None);
    old.state = JobState::Interrupted;
    store::save_current(&work, &old).unwrap();
    store::save_job_file(&work, STAGE, &old.id, "applied", b"done").unwrap();
    let mut stranger = Criterion::spawn();
    record_reused_legacy_group(&work, &old, stranger.0.id());
    age(&work, &old.id, 1_000);
    let mut keep = old.clone();
    for index in 0..4 {
        keep = running_record(STAGE, None);
        keep.state = JobState::Passed;
        store::save_current(&work, &keep).unwrap();
        store::save_job_file(&work, STAGE, &keep.id, "applied", b"done").unwrap();
        age(&work, &keep.id, 2_000 + index);
    }
    // Pruning can precede recovery after an upgrade; protect evidence before a
    // pending-retirement report has even been written.
    store::prune_jobs(&work, STAGE, &keep.id).unwrap();
    assert_eq!(
        store::load_uncontrolled_groups(&work, STAGE, &old.id).unwrap(),
        [stranger.0.id()]
    );

    assert!(recover_after_restart(&work).unwrap().is_empty());
    assert_eq!(
        store::load_current(&work, STAGE).unwrap(),
        Some(keep.clone())
    );
    assert!(!report(&work, &old).is_empty());
    age(&work, &old.id, 1_000);
    store::prune_jobs(&work, STAGE, &keep.id).unwrap();
    assert_eq!(
        store::load_job_record(&work, STAGE, &old.id).unwrap(),
        Some(old)
    );
    assert!(stranger.0.try_wait().unwrap().is_none());
}

#[test]
fn recovery_reports_legacy_groups_even_after_the_job_result_was_saved() {
    let (_directory, work) = state_root();
    let mut record = running_record(STAGE, None);
    record.state = JobState::Passed;
    store::save_current(&work, &record).unwrap();
    let mut stranger = Criterion::spawn();
    record_legacy_group(&work, &record, stranger.0.id());

    assert!(recover_after_restart(&work).unwrap().is_empty());
    assert_eq!(
        store::load_current(&work, STAGE).unwrap(),
        Some(record.clone())
    );
    assert!(!report(&work, &record).is_empty());
    assert!(stranger.0.try_wait().unwrap().is_none());
}

fn age(work: &Path, job: &str, seconds: u64) {
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
    File::open(store::job_dir(work, STAGE, job).unwrap())
        .unwrap()
        .set_times(FileTimes::new().set_modified(time))
        .unwrap();
}
