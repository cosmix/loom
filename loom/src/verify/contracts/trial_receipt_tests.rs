//! Which recorded trial a filing is checked against: the newest passing
//! contracts trial by submission order, never a failed, withdrawn or plain
//! check, and never a malformed receipt.

use chrono::Utc;

use super::*;
use crate::host_verify::{new_job_id, JobResult};

const STAGE: &str = "stage-a";

/// Leave job `order` of `kind` in `state`, with a result that `passed` and
/// `receipt` saved as its `trial.json`.
fn job(
    work_dir: &Path,
    order: u64,
    contracts: bool,
    state: JobState,
    passed: bool,
    receipt: &[u8],
) {
    let record = JobRecord {
        id: new_job_id(),
        submission_order: order,
        stage_id: STAGE.to_string(),
        session_id: "session-1".to_string(),
        kind: JobKind::Check,
        head: "a".repeat(40),
        tree_digest: "digest".to_string(),
        no_cache: false,
        criteria: None,
        contracts,
        state,
        requests: Vec::new(),
        queued_at: Utc::now(),
        started_at: None,
        finished_at: None,
        pid: None,
        detail: None,
        at_base: false,
    };
    job_store::save_current(work_dir, &record).unwrap();
    let result = JobResult {
        job_id: record.id.clone(),
        head: record.head.clone(),
        passed,
        criteria: Vec::new(),
        error: None,
        policy_hash: None,
    };
    job_store::save_result(work_dir, STAGE, &result).unwrap();
    job_store::save_job_file(work_dir, STAGE, &record.id, RECEIPT_FILE, receipt).unwrap();
}

fn receipt(content: &str) -> TrialReceipt {
    TrialReceipt {
        head: "a".repeat(40),
        files: vec![TrialFile {
            path: "tests/x_contract.rs".to_string(),
            sha256: sha256_hex(content.as_bytes()),
        }],
    }
}

fn bytes(content: &str) -> Vec<u8> {
    serde_json::to_vec(&receipt(content)).unwrap()
}

#[test]
fn the_newest_passing_contracts_trial_is_the_receipt() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    job(work, 2, true, JobState::Passed, true, &bytes("second"));
    job(work, 1, true, JobState::Passed, true, &bytes("first"));
    job(work, 3, true, JobState::Failed, false, &bytes("failed"));
    job(
        work,
        4,
        false,
        JobState::Passed,
        true,
        &bytes("plain check"),
    );
    job(
        work,
        5,
        true,
        JobState::Cancelled,
        true,
        &bytes("superseded"),
    );
    job(work, 6, true, JobState::Passed, true, b"{ not a receipt");

    let loaded = load_receipt(work, STAGE).unwrap();

    assert_eq!(loaded, Some(receipt("second")));
}

#[test]
fn a_stage_without_jobs_has_no_receipt() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(load_receipt(tmp.path(), STAGE).unwrap(), None);
}

/// A trial of A and B covers a filing of A and B only: A alone may need B,
/// and a file the trial never ran is no more covered than other bytes.
#[test]
fn a_filing_must_name_the_trialled_set_exactly() {
    let head = "a".repeat(40);
    let pair = |file: &str, sha: &str| (file.to_string(), sha.repeat(64));
    let receipt = TrialReceipt {
        head: head.clone(),
        files: [("a.rs", "1"), ("b.rs", "2")]
            .iter()
            .map(|(path, sha)| TrialFile {
                path: path.to_string(),
                sha256: sha.repeat(64),
            })
            .collect(),
    };
    let both = [pair("a.rs", "1"), pair("b.rs", "2")];
    assert_eq!(receipt_covers(Some(&receipt), &head, &both), Ok(()));
    let subset = receipt_covers(Some(&receipt), &head, &both[..1]).expect_err("a subset");
    assert!(
        subset.contains("loom check")
            && subset.contains("--replacement")
            && subset.contains("the same --replacement values as the trial"),
        "{subset}"
    );
    let superset = [pair("a.rs", "1"), pair("b.rs", "2"), pair("c.rs", "3")];
    assert!(receipt_covers(Some(&receipt), &head, &superset).is_err());
    let repeated = [pair("a.rs", "1"), pair("a.rs", "1")];
    assert!(receipt_covers(Some(&receipt), &head, &repeated).is_err());
}

#[test]
fn files_cover_ignores_the_head_and_still_checks_every_file_and_its_bytes() {
    let receipt = TrialReceipt {
        head: "old-head".to_string(),
        files: vec![TrialFile {
            path: "tests/x.rs".to_string(),
            sha256: "aa".to_string(),
        }],
    };
    let pair = |sha: &str| vec![("tests/x.rs".to_string(), sha.to_string())];
    assert_eq!(receipt_files_cover(&receipt, &pair("aa")), Ok(()));
    assert!(receipt_files_cover(&receipt, &pair("bb")).is_err());
    assert!(receipt_files_cover(&receipt, &[]).is_err());
    assert!(receipt_covers(Some(&receipt), "new-head", &pair("aa")).is_err());
}
