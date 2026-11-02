use super::*;

const ORDER_FILE: &str = ".submission-order";

/// Reserve the next per-stage order while the caller holds the stage lock.
/// The counter is published before the job: a crash can leave a gap, never
/// reuse an order already visible in a job record.
pub(crate) fn next_submission_order(work_dir: &Path, stage_id: &str) -> Result<u64> {
    let rel = stage_rel(stage_id)?.join(ORDER_FILE);
    let stored = match read_in(work_dir, &rel, 32)? {
        Some(bytes) => std::str::from_utf8(&bytes)?
            .parse::<u64>()
            .context("invalid host job submission order")?,
        None => {
            ensure!(
                highest_recorded_order(work_dir, stage_id)? == 0,
                "host job submission counter is missing for ordered jobs"
            );
            0
        }
    };
    let next = stored
        .checked_add(1)
        .context("host job submission order exhausted")?;
    write_atomic(&open_root(work_dir)?, &rel, next.to_string().as_bytes())?;
    Ok(next)
}

fn highest_recorded_order(work_dir: &Path, stage_id: &str) -> Result<u64> {
    let mut highest = 0;
    for job_id in job_ids(work_dir, stage_id)? {
        if let Some(record) = load_job_record(work_dir, stage_id, &job_id)? {
            ensure!(
                record.id == job_id && record.stage_id == stage_id,
                "host job record identity does not match its directory"
            );
            highest = highest.max(record.submission_order);
        }
    }
    Ok(highest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_verify::submit::{submit, Submission, SubmitRequest};
    use crate::host_verify::{JobKind, JobState};
    use chrono::Utc;

    #[test]
    fn submission_order_survives_pruning_and_restart() {
        let temp = tempfile::TempDir::new().unwrap();
        let work = temp.path().join("work");
        std::fs::create_dir(&work).unwrap();
        let head = "a".repeat(40);
        let digest = "b".repeat(64);
        for expected in 1..=5 {
            let request = SubmitRequest {
                stage_id: "stage-a",
                session_id: "session-a",
                request_id: "request-a",
                kind: JobKind::Check,
                head: &head,
                tree_digest: &digest,
                no_cache: false,
                criteria: None,
            };
            let Submission::Queued(mut job) = submit(&work, &request, Utc::now()).unwrap() else {
                panic!("expected queued job");
            };
            assert_eq!(job.submission_order, expected);
            job.state = JobState::Passed;
            save_current(&work, &job).unwrap();
            save_job_file(&work, "stage-a", &job.id, "applied", b"done").unwrap();
        }
        assert!(job_ids(&work, "stage-a").unwrap().len() <= 3);
        let _lock = lock_stage(&work, "stage-a").unwrap();
        assert_eq!(next_submission_order(&work, "stage-a").unwrap(), 6);
    }
}
