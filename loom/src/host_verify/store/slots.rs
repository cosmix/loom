//! Atomic job copies and current/waiting stage slots.
use super::*;

const PENDING_FILE: &str = "pending.json";

pub(super) fn save_slot(work_dir: &Path, record: &JobRecord, slot: Option<&str>) -> Result<()> {
    let job = job_rel(&record.stage_id, &record.id)?;
    let target = slot
        .map(|name| stage_rel(&record.stage_id).map(|dir| dir.join(name)))
        .transpose()?;
    let copy = job.join(RECORD_FILE);
    let root = open_root(work_dir)?;
    if let Some(target) = &target {
        refuse_unsafe(&root, target)?;
    }
    refuse_unsafe(&root, &copy)?;
    create_dirs(&root, &job)?;
    let bytes = serde_json::to_vec_pretty(record).context("cannot encode the job record")?;
    write_atomic(&root, &copy, &bytes)?;
    if let Some(target) = target {
        write_atomic(&root, &target, &bytes)?;
    }
    Ok(())
}

/// The job copy is written first, and wins over an older slot pointer.
pub(crate) fn load_pending(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>> {
    let pointer: Option<JobRecord> = read_json(
        work_dir,
        &stage_rel(stage_id)?.join(PENDING_FILE),
        RECORD_MAX_BYTES,
    )?;
    pointer
        .map(|record| Ok(load_job_record(work_dir, stage_id, &record.id)?.unwrap_or(record)))
        .transpose()
}

pub(crate) fn save_pending(work_dir: &Path, record: &JobRecord) -> Result<()> {
    save_slot(work_dir, record, Some(PENDING_FILE))
}

pub(crate) fn save_job_copy(work_dir: &Path, record: &JobRecord) -> Result<()> {
    save_slot(work_dir, record, None)
}

/// Call only under the stage lock; an absent slot is fine.
pub(crate) fn clear_pending(work_dir: &Path, stage_id: &str) -> Result<()> {
    let rel = stage_rel(stage_id)?.join(PENDING_FILE);
    safe_fs::safe_remove_in_workdir(open_root(work_dir)?.as_raw_fd(), &rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_verify::submit::submit;
    use crate::host_verify::tests_submit::{queued, request, state_root, HEAD_A, STAGE};
    use chrono::Utc;

    #[test]
    fn a_waiting_record_is_saved_beside_current_with_its_job_copy() {
        let (_tmp, work) = state_root();
        let current = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());
        let mut waiting = current.clone();
        waiting.id = "22222222222222222222222222222222".into();
        save_pending(&work, &waiting).unwrap();
        assert_eq!(load_current(&work, STAGE).unwrap(), Some(current));
        assert_eq!(load_pending(&work, STAGE).unwrap(), Some(waiting.clone()));
        assert_eq!(
            load_job_record(&work, STAGE, &waiting.id).unwrap(),
            Some(waiting)
        );
    }

    #[test]
    fn the_job_copy_wins_over_a_stale_pending_file() {
        let (_tmp, work) = state_root();
        let mut waiting = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());
        save_pending(&work, &waiting).unwrap();
        waiting.state = JobState::Cancelled;
        save_job_copy(&work, &waiting).unwrap();
        assert_eq!(load_pending(&work, STAGE).unwrap(), Some(waiting));
    }

    #[test]
    fn a_symlinked_pending_file_is_refused() {
        let (tmp, work) = state_root();
        let waiting = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());
        let target = tmp.path().join("outside");
        std::fs::write(&target, "untouched").unwrap();
        std::os::unix::fs::symlink(&target, stage_dir(&work, STAGE).unwrap().join(PENDING_FILE))
            .unwrap();
        assert!(save_pending(&work, &waiting).is_err());
        assert!(load_pending(&work, STAGE).is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "untouched");
    }

    #[test]
    fn clearing_an_absent_pending_file_succeeds() {
        let (_tmp, work) = state_root();
        let _lock = lock_stage(&work, STAGE).unwrap();
        clear_pending(&work, STAGE).unwrap();
    }
}
