//! Contracts for E2: a replacement trial admits a filing only when it passed,
//! settled, on the tree the stage holds when the dispute is filed.

use std::path::{Path, PathBuf};

use chrono::Utc;
use tempfile::TempDir;

use crate::daemon::ContractRunReport;
use crate::fs::work_dir::WorkDir;
use crate::host_verify::{digest, new_job_id, store, JobKind, JobRecord, JobResult, JobState};
use crate::models::dispute::{DisputeKind, FileReplacement};
use crate::models::stage::{Stage, StageStatus};
use crate::stage_service::{handle_file_dispute, Response};
use crate::utils::sha256_hex;
use crate::verify::contracts::store::{
    write_freeze, FreezeRecord, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use crate::verify::contracts::test_support::{
    contract, contract_worktree, pinned, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID,
};
use crate::verify::contracts::trial::{load_receipt, TrialFile, TrialReceipt, RECEIPT_FILE};
use crate::verify::transitions::{load_stage, save_stage};

const STAGE: &str = "stage-trial";
const REPLACEMENT: &str = "#[test]\nfn rejects_x() { assert!(false, \"replaced\"); }\n";

/// Save a contracts check of `head` and `tree_digest` in `state`, a passing
/// result, and a receipt of `REPLACEMENT` at the same head and digest.
fn record_trial(work_dir: &Path, state: JobState, head: &str, tree_digest: &str) {
    let record = JobRecord {
        id: new_job_id(),
        submission_order: 1,
        stage_id: STAGE.to_string(),
        session_id: "session-1".to_string(),
        kind: JobKind::Check,
        head: head.to_string(),
        tree_digest: tree_digest.to_string(),
        no_cache: false,
        criteria: None,
        contracts: true,
        at_base: false,
        state,
        requests: Vec::new(),
        queued_at: Utc::now(),
        started_at: Some(Utc::now()),
        finished_at: None,
        pid: None,
        detail: None,
    };
    store::save_current(work_dir, &record).unwrap();
    let result = JobResult {
        job_id: record.id.clone(),
        head: head.to_string(),
        passed: true,
        criteria: Vec::new(),
        error: None,
        policy_hash: None,
    };
    store::save_result(work_dir, STAGE, &result).unwrap();
    let receipt = TrialReceipt {
        head: head.to_string(),
        tree_digest: tree_digest.to_string(),
        files: vec![TrialFile {
            path: CONTRACT_FILE.to_string(),
            sha256: sha256_hex(REPLACEMENT.as_bytes()),
        }],
    };
    let bytes = serde_json::to_vec(&receipt).unwrap();
    store::save_job_file(work_dir, STAGE, &record.id, RECEIPT_FILE, &bytes).unwrap();
}

/// An executing plan v2 stage whose one contract is frozen over
/// `CONTRACT_CONTENT`. Returns the temp root, the work dir and the worktree.
fn frozen_stage() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let workspace = WorkDir::new(tmp.path()).unwrap();
    workspace.initialize().unwrap();
    let work_dir = workspace.root().to_path_buf();
    let worktree = contract_worktree(tmp.path(), STAGE);
    let report = ContractRunReport {
        contract_id: CONTRACT_ID.to_string(),
        adapter: Some("cargo-test".to_string()),
        outcome: "failed".to_string(),
        exit_code: Some(101),
    };
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: STAGE.to_string(),
        session_id: "session-contract".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: vec![FrozenFile {
            path: CONTRACT_FILE.to_string(),
            sha256: sha256_hex(CONTRACT_CONTENT),
        }],
        contracts: vec![FrozenContract {
            file: Some(CONTRACT_FILE.to_string()),
            ..FrozenContract::from(&report)
        }],
    };
    let frozen = [(CONTRACT_FILE.to_string(), CONTRACT_CONTENT.to_vec())];
    write_freeze(&work_dir, &record, &frozen).unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        name: "Trial".to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        contracts: vec![contract()],
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();
    (tmp, work_dir, worktree)
}

#[test]
fn a_running_trial_with_a_passing_result_is_no_receipt() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    record_trial(work, JobState::Running, &"a".repeat(40), "digest");
    let receipt = load_receipt(work, STAGE).unwrap();
    assert_eq!(
        receipt, None,
        "a Running job's passing result counted before the runner settled it"
    );
}

#[test]
fn a_trial_on_another_dirty_tree_does_not_admit_the_replacement() {
    let (_tmp, work_dir, worktree) = frozen_stage();
    let git = pinned(&worktree);
    let head = digest::head_in(&git).unwrap();
    let tree_digest = digest::tree_digest_in(&git).unwrap();
    record_trial(&work_dir, JobState::Passed, &head, &tree_digest);
    let extra = worktree.join("src/extra.rs");
    std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
    std::fs::write(&extra, "pub fn extra() {}\n").unwrap();
    assert_ne!(
        digest::tree_digest_in(&git).unwrap(),
        tree_digest,
        "the untracked edit changes the worktree digest"
    );
    let kind = DisputeKind::Contract {
        contract_ids: vec![CONTRACT_ID.to_string()],
        replacements: vec![FileReplacement {
            file: CONTRACT_FILE.to_string(),
            content: REPLACEMENT.to_string(),
            base_sha256: None,
        }],
    };
    let response = handle_file_dispute(&work_dir, STAGE, kind, "wrong".to_string(), None).unwrap();
    match response {
        Response::Error { message } => {
            assert!(message.contains("different tree"), "{message}")
        }
        other => panic!("a trial of another tree admitted the filing: {other:?}"),
    }
    let stage = load_stage(STAGE, &work_dir).unwrap();
    assert_eq!(stage.tally.contract_disputes, 0, "no dispute is spent");
}
