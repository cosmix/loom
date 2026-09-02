use super::*;
use crate::daemon::handle_dispute_criteria;
use crate::fs::stage_request::{append_to_spool, read_pending, spool_path};
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::DisputeRequest;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::transitions::{load_stage, save_stage};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// An initialized `.loom/work` plus a worktree directory for `stage_id`, with the
/// stage saved in `status` and `acceptance_len` acceptance criteria.
fn setup(
    stage_id: &str,
    status: StageStatus,
    acceptance_len: usize,
) -> (TempDir, PathBuf, PathBuf) {
    let temp = TempDir::new().unwrap();
    let wd = WorkDir::new(temp.path()).unwrap();
    wd.initialize().unwrap();
    let work_dir = wd.root().to_path_buf();

    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = status;
    for i in 0..acceptance_len {
        stage
            .acceptance
            .push(AcceptanceCriterion::Simple(format!("echo {i}")));
    }
    save_stage(&stage, &work_dir).unwrap();

    let worktree_root = temp.path().join(".worktrees").join(stage_id);
    std::fs::create_dir_all(&worktree_root).unwrap();
    (temp, work_dir, worktree_root)
}

fn dispute_request(criterion_index: usize) -> StageRequest {
    StageRequest::Dispute {
        criterion_index,
        reason: "criterion names a binary this stage never builds".to_string(),
        evidence_commit: Some("abc1234".to_string()),
        failure_output: Some("loom: command not found".to_string()),
    }
}

/// The single dispute on disk for `stage_id`, with `created_at` normalised
/// away so two runs are comparable.
fn only_dispute_on_disk(work_dir: &Path, stage_id: &str) -> DisputeRequest {
    let dir = work_dir.join("disputes").join(stage_id);
    let mut ids: Vec<u32> = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name();
        if let Some(id) = name.to_str().and_then(|n| n.parse::<u32>().ok()) {
            ids.push(id);
        }
    }
    assert_eq!(ids.len(), 1, "expected exactly one dispute in {dir:?}");
    let id = ids.pop().unwrap();

    let content = std::fs::read_to_string(dir.join(id.to_string()).join("request.md")).unwrap();
    let frontmatter = content.split("---").nth(1).unwrap();
    let mut record: DisputeRequest = serde_yaml::from_str(frontmatter).unwrap();
    record.created_at = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
    record
}

#[test]
fn a_spooled_block_lands_the_stage_in_blocked_and_empties_the_spool() {
    let (_temp, work_dir, worktree_root) = setup("build-api", StageStatus::Executing, 1);
    append_to_spool(
        &worktree_root,
        &StageRequest::Block {
            reason: "criterion 0 needs a binary this plan never builds".to_string(),
        },
    )
    .unwrap();

    let outcome = drain_requests(&work_dir, "build-api", &worktree_root).unwrap();

    assert_eq!(
        outcome,
        DrainOutcome {
            applied: 1,
            skipped: 0
        }
    );
    let stage = load_stage("build-api", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(
        stage.close_reason.as_deref(),
        Some("criterion 0 needs a binary this plan never builds")
    );
    assert!(read_pending(&worktree_root).unwrap().is_empty());
}

#[test]
fn a_spooled_dispute_produces_the_same_on_disk_dispute_as_the_rpc_path() {
    let (_spooled, spooled_work, worktree_root) = setup("stage-disp", StageStatus::Executing, 3);
    append_to_spool(&worktree_root, &dispute_request(1)).unwrap();
    let outcome = drain_requests(&spooled_work, "stage-disp", &worktree_root).unwrap();
    assert_eq!(
        outcome,
        DrainOutcome {
            applied: 1,
            skipped: 0
        }
    );

    // The same dispute filed the way a caller that CAN reach the daemon files
    // it, in an independent work dir.
    let (_direct, direct_work, _) = setup("stage-disp", StageStatus::Executing, 3);
    handle_dispute_criteria(
        &direct_work,
        "stage-disp",
        1,
        "criterion names a binary this stage never builds".to_string(),
        Some("abc1234".to_string()),
        Some("loom: command not found".to_string()),
    )
    .unwrap();

    assert_eq!(
        only_dispute_on_disk(&spooled_work, "stage-disp"),
        only_dispute_on_disk(&direct_work, "stage-disp")
    );
    let spooled_stage = load_stage("stage-disp", &spooled_work).unwrap();
    let direct_stage = load_stage("stage-disp", &direct_work).unwrap();
    assert_eq!(spooled_stage.status, StageStatus::NeedsAdjudication);
    assert_eq!(spooled_stage.status, direct_stage.status);
    assert_eq!(spooled_stage.dispute_count, direct_stage.dispute_count);
}

#[test]
fn a_malformed_line_is_counted_and_the_request_after_it_still_lands() {
    let (_temp, work_dir, worktree_root) = setup("build-api", StageStatus::Executing, 1);
    let path = spool_path(&worktree_root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ not a request\n").unwrap();
    append_to_spool(
        &worktree_root,
        &StageRequest::Block {
            reason: "queued after the corrupt line".to_string(),
        },
    )
    .unwrap();

    let outcome = drain_requests(&work_dir, "build-api", &worktree_root).unwrap();

    assert_eq!(
        outcome,
        DrainOutcome {
            applied: 1,
            skipped: 1
        }
    );
    let stage = load_stage("build-api", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Blocked);
    assert_eq!(
        stage.close_reason.as_deref(),
        Some("queued after the corrupt line")
    );
}

#[test]
fn a_refused_request_is_counted_and_discarded_not_redelivered_forever() {
    // A Completed stage cannot be blocked. That is an ANSWER from the handler,
    // so it must not wedge the spool the way an I/O failure would.
    let (_temp, work_dir, worktree_root) = setup("build-api", StageStatus::Completed, 1);
    append_to_spool(
        &worktree_root,
        &StageRequest::Block {
            reason: "too late".to_string(),
        },
    )
    .unwrap();

    let outcome = drain_requests(&work_dir, "build-api", &worktree_root).unwrap();

    assert_eq!(
        outcome,
        DrainOutcome {
            applied: 0,
            skipped: 1
        }
    );
    assert_eq!(
        load_stage("build-api", &work_dir).unwrap().status,
        StageStatus::Completed
    );
    assert!(
        read_pending(&worktree_root).unwrap().is_empty(),
        "a refusal is final; redelivering it would wedge every request behind it"
    );
}

#[test]
fn a_spool_for_a_stage_that_no_longer_exists_errors_without_panicking() {
    let (_temp, work_dir, worktree_root) = setup("build-api", StageStatus::Executing, 1);
    // Stage files carry a topological-depth prefix, so drop the directory
    // rather than guessing at the filename.
    std::fs::remove_dir_all(work_dir.join("stages")).unwrap();
    append_to_spool(
        &worktree_root,
        &StageRequest::Block {
            reason: "the stage file is gone".to_string(),
        },
    )
    .unwrap();

    let error = drain_requests(&work_dir, "build-api", &worktree_root).unwrap_err();

    assert!(
        !format!("{error:#}").is_empty(),
        "the failure must be reported, not panicked"
    );
    assert_eq!(
        read_pending(&worktree_root).unwrap().len(),
        1,
        "an I/O-level failure keeps the request pending for the next tick"
    );
}

#[test]
fn a_worktree_with_no_spool_is_a_silent_no_op() {
    let (_temp, work_dir, worktree_root) = setup("build-api", StageStatus::Executing, 1);

    let outcome = drain_requests(&work_dir, "build-api", &worktree_root).unwrap();

    assert_eq!(outcome, DrainOutcome::default());
    assert_eq!(
        load_stage("build-api", &work_dir).unwrap().status,
        StageStatus::Executing
    );
}
