//! Submitting a contracts check (`submit_contracts`): its own selection,
//! never joined to or by another job, and a trial's replacement files saved
//! with the job.

use chrono::Utc;

use super::store::load_job_file;
use super::submit::{submit, submit_contracts, Submission, REPLACEMENTS_FILE};
use super::tests_submit::{queued, request, state_root, subset, superseded, HEAD_A, STAGE};
use super::JobKind;
use crate::models::dispute::FileReplacement;

fn replacement(content: &str) -> Vec<FileReplacement> {
    vec![FileReplacement {
        file: "tests/x_contract.rs".to_string(),
        content: content.to_string(),
        base_sha256: None,
    }]
}

#[test]
fn a_contracts_check_neither_joins_nor_is_joined_by_a_criteria_check() {
    let (_dir, work) = state_root();
    let full = queued(submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap());
    let contracts = submit_contracts(&work, &request("r2", HEAD_A, false), &[], Utc::now());
    let (cancelled, contracts) = superseded(contracts.unwrap());
    assert_eq!(cancelled.id, full.id);
    assert!(contracts.contracts && !full.contracts);

    // A check of the same session and tree waits behind the contracts run
    // as it waits behind another check, as a job of its own.
    let check = submit(&work, &request("r3", HEAD_A, false), Utc::now()).unwrap();
    match check {
        Submission::Waiting { pending, behind } => {
            assert_eq!(
                behind, contracts.id,
                "a full check is not answered by contracts"
            );
            assert!(!pending.contracts);
        }
        other => panic!("expected a waiting check, got {other:?}"),
    }
}

#[test]
fn a_trial_is_never_joined_and_keeps_its_own_replacement_files() {
    let (_dir, work) = state_root();
    let first = submit_contracts(
        &work,
        &request("r1", HEAD_A, false),
        &replacement("one"),
        Utc::now(),
    );
    let first = queued(first.unwrap());
    let second = submit_contracts(
        &work,
        &request("r2", HEAD_A, false),
        &replacement("two"),
        Utc::now(),
    );
    let (cancelled, second) = superseded(second.unwrap());
    assert_eq!(
        cancelled.id, first.id,
        "another trial supersedes, never joins"
    );
    for (job, content) in [(&first, "one"), (&second, "two")] {
        let bytes = load_job_file(&work, STAGE, &job.id, REPLACEMENTS_FILE)
            .unwrap()
            .expect("the replacement files are saved with the job");
        let saved: Vec<FileReplacement> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved, replacement(content));
    }
}

#[test]
fn a_contracts_check_with_criteria_or_of_another_kind_is_refused() {
    let (_dir, work) = state_root();
    let with_criteria = submit_contracts(&work, &subset("r1", &[0]), &[], Utc::now()).unwrap();
    assert!(matches!(with_criteria, Submission::Refused(reason) if reason.contains("criteria")));
    let completion = super::submit::SubmitRequest {
        kind: JobKind::Complete,
        ..request("r2", HEAD_A, false)
    };
    let refused = submit_contracts(&work, &completion, &[], Utc::now()).unwrap();
    assert!(matches!(refused, Submission::Refused(_)), "{refused:?}");
}
