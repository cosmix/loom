//! A contract dispute carrying replacements is admitted only while a passing
//! trial of the same bytes at the worktree's HEAD and on its current tree is
//! on record; a filing without replacements needs none. Nothing is spent on a
//! refusal.

use chrono::Utc;

use super::*;
use crate::git::worktree::WorktreeGit;
use crate::host_verify::{new_job_id, store, JobKind, JobRecord, JobResult, JobState};
use crate::verify::contracts::trial::{current_tree, TrialFile, TrialReceipt, RECEIPT_FILE};

/// The commit and tree digest the stage worktree holds now.
pub(super) fn worktree_state(work_dir: &Path) -> (String, String) {
    let repo_root = work_dir.parent().unwrap().parent().unwrap();
    let worktree = repo_root.join(".worktrees").join(STAGE);
    let git = WorktreeGit::pinned(repo_root, &worktree).unwrap();
    let stage = load_stage(STAGE, work_dir).unwrap();
    current_tree(&git, &stage).unwrap()
}

/// Leave the receipt of a passing contracts check at `head` on the tree
/// `tree_digest` that hashed `files`, as the host job of a trial saves it.
fn record_trial(work_dir: &Path, (head, tree_digest): (&str, &str), files: &[(&str, &str)]) {
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
        state: JobState::Passed,
        requests: Vec::new(),
        queued_at: Utc::now(),
        started_at: None,
        finished_at: None,
        pid: None,
        detail: None,
        at_base: false,
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
        files: files
            .iter()
            .map(|(path, content)| TrialFile {
                path: path.to_string(),
                sha256: sha256_hex(content.as_bytes()),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&receipt).unwrap();
    store::save_job_file(work_dir, STAGE, &record.id, RECEIPT_FILE, &bytes).unwrap();
}

/// File `kind` after a passing trial of exactly its replacements at HEAD on
/// the worktree as it is.
pub(super) fn file_trialed(work_dir: &Path, kind: DisputeKind) -> Response {
    let DisputeKind::Contract { replacements, .. } = &kind else {
        panic!("not a contract dispute");
    };
    let files: Vec<(&str, &str)> = replacements
        .iter()
        .map(|replacement| (replacement.file.as_str(), replacement.content.as_str()))
        .collect();
    let (head, tree_digest) = worktree_state(work_dir);
    record_trial(work_dir, (&head, &tree_digest), &files);
    file(work_dir, kind)
}

const EDITED: &str = "#[test]\nfn rejects_x() { assert!(false, \"edited\"); }\n";

#[test]
fn a_replacement_without_a_recorded_trial_is_refused() {
    let (_tmp, work_dir) = frozen_stage();

    assert_refused(
        &work_dir,
        dispute(Some(EDITED), None),
        &[
            "no passing replacement trial",
            "loom check",
            "--replacement",
        ],
    );
}

#[test]
fn a_trial_of_other_bytes_does_not_admit_the_replacement() {
    let (_tmp, work_dir) = frozen_stage();
    let (head, tree_digest) = worktree_state(&work_dir);
    record_trial(
        &work_dir,
        (&head, &tree_digest),
        &[(CONTRACT_FILE, "fn other_bytes() {}\n")],
    );

    assert_refused(
        &work_dir,
        dispute(Some(EDITED), None),
        &["did not run these replacement bytes", CONTRACT_FILE],
    );
}

#[test]
fn a_trial_at_another_head_does_not_admit_the_replacement() {
    let (_tmp, work_dir) = frozen_stage();
    let (_, tree_digest) = worktree_state(&work_dir);
    record_trial(
        &work_dir,
        ("0000000000000000000000000000000000000000", &tree_digest),
        &[(CONTRACT_FILE, EDITED)],
    );

    assert_refused(
        &work_dir,
        dispute(Some(EDITED), None),
        &["the passing trial ran at HEAD 0000000000"],
    );
}

#[test]
fn an_edit_after_the_trial_does_not_admit_the_replacement() {
    let (_tmp, work_dir) = frozen_stage();
    let (head, tree_digest) = worktree_state(&work_dir);
    record_trial(&work_dir, (&head, &tree_digest), &[(CONTRACT_FILE, EDITED)]);
    let extra = worktree_file(&work_dir, "src/extra.rs");
    std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
    std::fs::write(&extra, "pub fn extra() {}\n").unwrap();

    assert_refused(&work_dir, dispute(Some(EDITED), None), &["different tree"]);
}

#[test]
fn a_matching_trial_admits_the_replacement() {
    let (_tmp, work_dir) = frozen_stage();

    assert!(matches!(
        file_trialed(&work_dir, dispute(Some(EDITED), None)),
        Response::DisputeCreated { .. }
    ));
    assert_eq!(disputes_spent(&work_dir), 1);
}
