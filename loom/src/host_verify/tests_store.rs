//! The host-job store: id validation, symlink refusal under a symlinked
//! state root, atomic records, the progress cap and job-directory pruning.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::Utc;
use tempfile::TempDir;

use super::store::{
    append_group, append_progress, job_dir, load_current, load_groups, load_job_record, lock_stage,
    next_submission_order, prune_jobs, read_progress, save_criterion_log, save_current,
    save_job_file, stage_dir, start_guarded_group,
};
use super::{new_job_id, CriterionOutcome, JobKind, JobRecord, JobState};
use crate::verify::criteria::GroupAnchor;

const STAGE: &str = "stage-a";

fn state_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let work = dir.path().join(".loom").join("work");
    fs::create_dir_all(&work).unwrap();
    (dir, work)
}

fn record(state: JobState) -> JobRecord {
    JobRecord {
        id: new_job_id(),
        submission_order: 0,
        stage_id: STAGE.to_string(),
        session_id: "session-a".to_string(),
        kind: JobKind::Check,
        head: "a".repeat(40),
        tree_digest: "b".repeat(64),
        no_cache: false,
        criteria: None,
        state,
        requests: vec!["r1".to_string()],
        queued_at: Utc::now(),
        started_at: None,
        finished_at: None,
        pid: None,
        detail: None,
    }
}

#[test]
fn old_job_json_defaults_to_unordered() {
    let job = record(JobState::Failed);
    let mut json = serde_json::to_value(&job).unwrap();
    json.as_object_mut().unwrap().remove("submission_order");

    let decoded: JobRecord = serde_json::from_value(json).unwrap();

    assert_eq!(decoded, job);
}

#[test]
fn submission_order_survives_restart_and_a_reserved_gap() {
    let (_dir, work) = state_root();
    {
        let _lock = lock_stage(&work, STAGE).unwrap();
        assert_eq!(next_submission_order(&work, STAGE).unwrap(), 1);
        assert_eq!(next_submission_order(&work, STAGE).unwrap(), 2);
    }
    // The first reservation never published a job, as after a crash.
    let mut published = record(JobState::Queued);
    published.submission_order = 2;
    save_current(&work, &published).unwrap();

    let _lock = lock_stage(&work, STAGE).unwrap();
    assert_eq!(next_submission_order(&work, STAGE).unwrap(), 3);
}

fn outcome(label: &str, summary: String) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed: true,
        summary,
        log: None,
    }
}

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).unwrap().next().is_none()
}

#[test]
fn ids_are_validated_before_any_path_is_built() {
    let (_dir, work) = state_root();
    let job = new_job_id();
    assert!(stage_dir(&work, "../x").is_err());
    assert!(stage_dir(&work, "").is_err());
    assert!(job_dir(&work, STAGE, "../../x").is_err());
    assert!(job_dir(&work, STAGE, &"A".repeat(32)).is_err());
    assert!(job_dir(&work, STAGE, &job[..31]).is_err());
    assert_eq!(
        job_dir(&work, STAGE, &job).unwrap(),
        work.join("verification").join(STAGE).join(&job)
    );
}

#[test]
fn a_symlinked_verification_directory_is_refused_with_nothing_created() {
    let (_dir, work) = state_root();
    let outside = TempDir::new().unwrap();
    symlink(outside.path(), work.join("verification")).unwrap();

    assert!(save_current(&work, &record(JobState::Queued)).is_err());
    assert!(super::store::lock_stage(&work, STAGE).is_err());
    assert!(is_empty_dir(outside.path()));
}

#[test]
fn a_symlinked_stage_or_job_directory_is_refused_with_nothing_created() {
    let (_dir, work) = state_root();
    let job = record(JobState::Queued);
    let outside = TempDir::new().unwrap();
    fs::create_dir_all(work.join("verification")).unwrap();
    symlink(outside.path(), work.join("verification").join(STAGE)).unwrap();
    assert!(save_current(&work, &job).is_err());
    assert!(is_empty_dir(outside.path()));

    fs::remove_file(work.join("verification").join(STAGE)).unwrap();
    fs::create_dir_all(stage_dir(&work, STAGE).unwrap()).unwrap();
    symlink(outside.path(), job_dir(&work, STAGE, &job.id).unwrap()).unwrap();
    assert!(save_current(&work, &job).is_err());
    assert!(is_empty_dir(outside.path()));
}

#[test]
fn a_symlinked_target_file_is_refused_and_left_alone() {
    let (_dir, work) = state_root();
    let job = record(JobState::Queued);
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("current.json");
    fs::write(&target, "keep").unwrap();
    fs::create_dir_all(stage_dir(&work, STAGE).unwrap()).unwrap();
    symlink(
        &target,
        stage_dir(&work, STAGE).unwrap().join("current.json"),
    )
    .unwrap();

    assert!(save_current(&work, &job).is_err());
    assert!(load_current(&work, STAGE).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
}

#[test]
fn a_symlinked_state_root_is_accepted() {
    let real = TempDir::new().unwrap();
    let links = TempDir::new().unwrap();
    let linked = links.path().join("work");
    symlink(real.path(), &linked).unwrap();
    let job = record(JobState::Queued);

    save_current(&linked, &job).unwrap();

    assert_eq!(load_current(&linked, STAGE).unwrap(), Some(job.clone()));
    assert!(real
        .path()
        .join("verification")
        .join(STAGE)
        .join(&job.id)
        .join("record.json")
        .is_file());
}

#[test]
fn a_saved_record_round_trips_beside_its_job_copy() {
    let (_dir, work) = state_root();
    assert_eq!(load_current(&work, STAGE).unwrap(), None);
    let mut job = record(JobState::Running);
    job.pid = Some(42);
    job.detail = Some("detail".to_string());

    save_current(&work, &job).unwrap();

    assert_eq!(load_current(&work, STAGE).unwrap(), Some(job.clone()));
    assert_eq!(
        load_job_record(&work, STAGE, &job.id).unwrap(),
        Some(job.clone())
    );
    let stage = stage_dir(&work, STAGE).unwrap();
    assert!(stage.join("current.json").is_file());
    assert!(stage.join(&job.id).join("record.json").is_file());
    let leftovers: Vec<_> = fs::read_dir(&stage)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn progress_past_two_mib_ends_with_one_truncation_marker() {
    let (_dir, work) = state_root();
    let job = record(JobState::Running);
    save_current(&work, &job).unwrap();
    // Three of these fit under 2 MiB, a fourth does not.
    let big = "x".repeat(600 * 1024);
    for index in 0..5 {
        let label = format!("acceptance-{index}");
        append_progress(&work, STAGE, &job.id, &outcome(&label, big.clone())).unwrap();
    }

    let progress = read_progress(&work, STAGE, &job.id).unwrap();
    let labels: Vec<&str> = progress.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "acceptance-0",
            "acceptance-1",
            "acceptance-2",
            "progress-truncated"
        ]
    );
    assert!(!progress[3].passed);
}

#[test]
fn groups_are_recorded_and_zero_or_one_never_listed() {
    let (_dir, work) = state_root();
    let job = record(JobState::Running);
    assert!(load_groups(&work, STAGE, &job.id).unwrap().is_empty());
    save_current(&work, &job).unwrap();
    for pgid in [0, 1, 4242, 4343] {
        append_group(&work, STAGE, &job.id, pgid).unwrap();
    }

    assert_eq!(load_groups(&work, STAGE, &job.id).unwrap(), [4242, 4343]);
}

#[test]
fn criterion_logs_and_job_files_validate_their_names() {
    let (_dir, work) = state_root();
    let job = record(JobState::Running);
    save_current(&work, &job).unwrap();

    let name = save_criterion_log(&work, STAGE, &job.id, "acceptance-0", b"out").unwrap();
    assert_eq!(name, "acceptance-0.log");
    assert!(save_criterion_log(&work, STAGE, &job.id, "../x", b"out").is_err());
    assert!(save_criterion_log(&work, STAGE, &job.id, "Upper", b"out").is_err());
    assert!(save_job_file(&work, STAGE, &job.id, ".lock", b"x").is_err());
    assert!(save_job_file(&work, STAGE, &job.id, "a/b", b"x").is_err());
    save_job_file(&work, STAGE, &job.id, "applied", b"done").unwrap();

    let dir = job_dir(&work, STAGE, &job.id).unwrap();
    assert_eq!(fs::read(dir.join("acceptance-0.log")).unwrap(), b"out");
    assert_eq!(
        super::store::load_job_file(&work, STAGE, &job.id, "applied").unwrap(),
        Some(b"done".to_vec())
    );
}

#[test]
fn an_oversized_criterion_log_is_cut_with_a_truncation_line() {
    let (_dir, work) = state_root();
    let job = record(JobState::Running);
    save_current(&work, &job).unwrap();
    let limit = crate::fs::safe_fs::MAX_LOG_BYTES;

    let name = save_criterion_log(
        &work,
        STAGE,
        &job.id,
        "goal-backward",
        &vec![b'y'; limit + 10],
    )
    .unwrap();

    let saved = fs::read(job_dir(&work, STAGE, &job.id).unwrap().join(name)).unwrap();
    assert!(saved.len() <= limit, "{}", saved.len());
    assert!(saved.ends_with(b"[truncated at 4 MiB]\n"));
}

/// Set a job directory's mtime to `secs` past the epoch.
fn age(work: &Path, job_id: &str, secs: u64) {
    let dir = fs::File::open(job_dir(work, STAGE, job_id).unwrap()).unwrap();
    dir.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

#[test]
fn pruning_keeps_the_three_newest_job_directories_and_the_current_one() {
    let (_dir, work) = state_root();
    let jobs: Vec<JobRecord> = (0..5).map(|_| record(JobState::Passed)).collect();
    for (index, job) in jobs.iter().enumerate() {
        save_current(&work, job).unwrap();
        save_job_file(&work, STAGE, &job.id, "applied", b"done").unwrap();
        age(&work, &job.id, 1_000 + index as u64);
    }

    prune_jobs(&work, STAGE, &jobs[0].id).unwrap();

    let kept: Vec<bool> = jobs
        .iter()
        .map(|job| job_dir(&work, STAGE, &job.id).unwrap().exists())
        .collect();
    assert_eq!(kept, [true, false, true, true, true]);
    assert!(stage_dir(&work, STAGE)
        .unwrap()
        .join("current.json")
        .is_file());
}

#[test]
fn pruning_keeps_an_old_terminal_job_without_an_applied_marker() {
    let (_dir, work) = state_root();
    let pending = record(JobState::Failed);
    save_current(&work, &pending).unwrap();
    age(&work, &pending.id, 1_000);
    let newer: Vec<JobRecord> = (0..4).map(|_| record(JobState::Passed)).collect();
    for (index, job) in newer.iter().enumerate() {
        save_current(&work, job).unwrap();
        save_job_file(&work, STAGE, &job.id, "applied", b"done").unwrap();
        age(&work, &job.id, 2_000 + index as u64);
    }

    prune_jobs(&work, STAGE, &newer[3].id).unwrap();

    assert!(job_dir(&work, STAGE, &pending.id).unwrap().exists());
}

#[test]
fn store_files_are_refused_as_job_files_or_criterion_logs() {
    let (_dir, work) = state_root();
    let job = record(JobState::Running);
    save_current(&work, &job).unwrap();

    for name in [
        "record.json",
        "result.json",
        "groups",
        "child.log",
        "progress.jsonl",
    ] {
        assert!(
            save_job_file(&work, STAGE, &job.id, name, b"x").is_err(),
            "{name}"
        );
    }
    assert!(save_criterion_log(&work, STAGE, &job.id, "child", b"x").is_err());
    assert_eq!(load_job_record(&work, STAGE, &job.id).unwrap(), Some(job));
}

#[test]
fn pruning_keeps_a_job_cancelled_after_its_start_before_its_pid_was_written() {
    let (_dir, work) = state_root();
    let mut started = record(JobState::Cancelled);
    started.started_at = Some(Utc::now());
    started.finished_at = Some(Utc::now());
    save_current(&work, &started).unwrap();
    age(&work, &started.id, 1_000);
    let newer: Vec<JobRecord> = (0..4).map(|_| record(JobState::Passed)).collect();
    for (index, job) in newer.iter().enumerate() {
        save_current(&work, job).unwrap();
        save_job_file(&work, STAGE, &job.id, "applied", b"done").unwrap();
        age(&work, &job.id, 2_000 + index as u64);
    }

    prune_jobs(&work, STAGE, &newer[3].id).unwrap();

    assert!(!job_dir(&work, STAGE, &newer[0].id).unwrap().exists());
    assert!(
        job_dir(&work, STAGE, &started.id).unwrap().exists(),
        "the directory the runner reads to retire the job was pruned"
    );
}

#[test]
fn pruning_keeps_an_old_job_with_a_live_group_guardian() {
    let (_dir, work) = state_root();
    let mut old = record(JobState::Cancelled);
    old.started_at = Some(Utc::now() - chrono::Duration::minutes(3));
    old.finished_at = Some(Utc::now() - chrono::Duration::minutes(2));
    save_current(&work, &old).unwrap();
    let (leader, address) = start_guarded_group(&work, STAGE, &old.id).unwrap();
    let _anchor = GroupAnchor::new(leader, Box::new(move || address.signal()));
    age(&work, &old.id, 1_000);
    let newer: Vec<JobRecord> = (0..4).map(|_| record(JobState::Passed)).collect();
    for (index, job) in newer.iter().enumerate() {
        save_current(&work, job).unwrap();
        save_job_file(&work, STAGE, &job.id, "applied", b"done").unwrap();
        age(&work, &job.id, 2_000 + index as u64);
    }

    prune_jobs(&work, STAGE, &newer[3].id).unwrap();

    assert!(job_dir(&work, STAGE, &old.id).unwrap().exists());
}
