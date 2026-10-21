//! Frozen contracts for stage daemon-completion: an in-session
//! `loom stage complete` relays a complete ticket and runs nothing; the daemon
//! applies a finished Complete job, completing the stage only for a passing
//! run of the stage's current committed content and recording a checkpoint
//! otherwise; a check never cancels a completion; subagents cannot relay
//! completion; a freeze request carries no session-made reports; capsules
//! drop the nested user-namespace opt-out.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "daemon_completion_contracts/support.rs"]
mod support;

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use loom::handoff::completion::policy::effective_policy_hash;
use loom::handoff::generator::load_trusted_session_checkpoint;
use loom::handoff::schema::{CompletionAttemptEvidence, CompletionPhase};
use loom::host_verify::apply::{apply_finished, Applied};
use loom::host_verify::digest::tree_digest;
use loom::host_verify::store::{load_current, save_current, save_result};
use loom::host_verify::submit::{submit, Submission, SubmitRequest};
use loom::host_verify::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use loom::models::stage::{Implementers, StageStatus, StageType};
use loom::plan::schema::{SandboxConfig, StageSandboxConfig};
use loom::relay::{decode_payload, RequestKind};
use loom::sandbox::{generate_settings_json, merge_config};
use loom::verify::transitions::load_stage;
use serde_json::json;
use serial_test::serial;

use support::{build_fixture, complete_record, state_root, Fixture, SESSION, STAGE};

const HEAD_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_B: &str = "cccccccccccccccccccccccccccccccccccccccc";
const DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// Save a finished Complete job for the worktree's HEAD and its result with
/// `outcomes`; the result passes when every outcome does.
fn finished_job(
    fx: &Fixture,
    state: JobState,
    outcomes: &[(&str, bool)],
    policy_hash: Option<String>,
) -> JobRecord {
    let digest = tree_digest(&fx.worktree).unwrap();
    let record = complete_record(state, &fx.head(), &digest);
    save_current(&fx.work_dir, &record).unwrap();
    let criteria = outcomes
        .iter()
        .map(|(label, passed)| CriterionOutcome {
            label: label.to_string(),
            passed: *passed,
            summary: format!("{label}: passed={passed}"),
            log: None,
        })
        .collect();
    let result = JobResult {
        job_id: record.id.clone(),
        head: record.head.clone(),
        passed: outcomes.iter().all(|(_, passed)| *passed),
        criteria,
        error: None,
        policy_hash,
    };
    save_result(&fx.work_dir, STAGE, &result).unwrap();
    record
}

/// The policy hash of stage-a as the daemon computes it now.
fn current_policy(fx: &Fixture) -> String {
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    effective_policy_hash(&stage, &fx.work_dir).unwrap()
}

/// The variant name of an apply outcome, for assertion messages.
fn variant(applied: &Applied) -> &'static str {
    match applied {
        Applied::Completed { .. } => "Completed",
        Applied::Recorded => "Recorded",
        Applied::FreezeAccepted { .. } => "FreezeAccepted",
        Applied::FreezeRefused(_) => "FreezeRefused",
        Applied::Ignored(_) => "Ignored",
    }
}

/// The latest attested completion attempt of session-a on stage-a.
fn latest_attempt(fx: &Fixture) -> CompletionAttemptEvidence {
    load_trusted_session_checkpoint(STAGE, SESSION, &fx.work_dir)
        .unwrap()
        .expect("a trusted completion checkpoint")
        .latest
        .expect("a latest completion attempt")
}

fn request<'a>(kind: JobKind, request_id: &'a str, head: &'a str) -> SubmitRequest<'a> {
    SubmitRequest {
        stage_id: STAGE,
        session_id: SESSION,
        request_id,
        kind,
        head,
        tree_digest: DIGEST,
        no_cache: false,
    }
}

#[test]
#[serial]
fn in_session_complete_relays_and_runs_nothing() {
    let fx = build_fixture(&["touch ran-in-session"]);
    let output = fx
        .session_loom()
        .args(["stage", "complete", STAGE])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stdout}\n{stderr}");
    let first = stdout.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("Host job: requested (complete, "),
        "{stdout}"
    );
    let last = stdout.lines().next_back().unwrap_or_default();
    assert!(last.starts_with("LOOM_RELAY_V1"), "{stdout}");
    assert!(!stdout.contains("LOOM_CONTROL_EVIDENCE_V1"), "{stdout}");

    let tickets: Vec<PathBuf> = fs::read_dir(&fx.scratch_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "req"))
        .collect();
    assert_eq!(tickets.len(), 1, "{tickets:?}");
    let ticket: serde_json::Value =
        serde_json::from_slice(&fs::read(&tickets[0]).unwrap()).unwrap();
    assert_eq!(ticket["kind"], "complete", "{ticket}");
    assert!(
        !fx.worktree.join("ran-in-session").exists(),
        "criteria ran in the session"
    );
}

#[test]
#[serial]
fn a_failed_complete_job_leaves_the_stage_executing() {
    let fx = build_fixture(&["true", "false"]);
    let outcomes = [("acceptance-0", true), ("acceptance-1", false)];
    let record = finished_job(&fx, JobState::Failed, &outcomes, None);

    let applied = apply_finished(&fx.work_dir, &record).unwrap();
    assert_eq!(variant(&applied), "Recorded");
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.completed_commit, None);

    let attempt = latest_attempt(&fx);
    assert_eq!(attempt.phase, CompletionPhase::ToolFailed);
    let failed = attempt
        .verification
        .criteria
        .iter()
        .find(|criterion| criterion.id == "acceptance-1")
        .expect("the checkpoint names acceptance-1");
    assert!(!failed.passed, "{:?}", attempt.verification);
}

#[test]
#[serial]
fn a_passing_job_for_the_current_head_completes() {
    let fx = build_fixture(&["true"]);
    let head = fx.head();
    let policy = Some(current_policy(&fx));
    let record = finished_job(&fx, JobState::Passed, &[("acceptance-0", true)], policy);

    let applied = apply_finished(&fx.work_dir, &record).unwrap();
    match &applied {
        Applied::Completed { commit } => assert_eq!(commit, &head),
        other => panic!("expected Applied::Completed, got {}", variant(other)),
    }
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert_eq!(stage.completed_commit.as_deref(), Some(head.as_str()));

    let again = apply_finished(&fx.work_dir, &record).unwrap();
    assert_eq!(variant(&again), "Ignored");
}

#[test]
#[serial]
fn a_passing_job_for_a_moved_head_is_not_completed() {
    let fx = build_fixture(&["true"]);
    let policy = Some(current_policy(&fx));
    let record = finished_job(&fx, JobState::Passed, &[("acceptance-0", true)], policy);
    let moved = fx.commit_change("later.txt");
    assert_ne!(moved, record.head);

    let applied = apply_finished(&fx.work_dir, &record).unwrap();
    assert_eq!(variant(&applied), "Recorded");
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.completed_commit, None);
    assert_eq!(latest_attempt(&fx).phase, CompletionPhase::DaemonRejected);
}

#[test]
fn a_check_never_cancels_a_completion() {
    let (_dir, w) = state_root();
    let now = Utc::now();
    let complete = match submit(&w, &request(JobKind::Complete, "r1", HEAD_A), now).unwrap() {
        Submission::Queued(record) => record,
        other => panic!("expected Submission::Queued, got {other:?}"),
    };
    let refused = submit(&w, &request(JobKind::Check, "r2", HEAD_B), now).unwrap();
    assert!(matches!(refused, Submission::Refused(_)), "{refused:?}");
    let current = load_current(&w, STAGE).unwrap().expect("a current record");
    assert_eq!(current.id, complete.id);
    assert_eq!(current.kind, JobKind::Complete);
    assert_eq!(current.state, JobState::Queued);

    match submit(&w, &request(JobKind::Check, "r3", HEAD_A), now).unwrap() {
        Submission::Joined(record) => assert_eq!(record.id, complete.id),
        other => panic!("expected Submission::Joined, got {other:?}"),
    }
}

#[test]
fn subagents_cannot_relay_completion() {
    assert!(RequestKind::Complete.is_control());
    assert!(!RequestKind::Check.is_control());
    for wire in ["complete", "check"] {
        let parsed: RequestKind = wire.parse().expect("the kind parses");
        assert_eq!(parsed.to_string(), wire);
    }
}

#[test]
fn freeze_request_carries_no_reports() {
    let kind = RequestKind::FreezeContracts;
    let reports_only = json!({"request": "freeze_contracts", "reports": []});
    assert!(decode_payload(kind, &reports_only).is_err());
    let beside = json!({
        "request": "freeze_contracts",
        "head": HEAD_A,
        "tree_digest": DIGEST,
        "reports": [],
    });
    assert!(decode_payload(kind, &beside).is_err());
    let bound = json!({"request": "freeze_contracts", "head": HEAD_A, "tree_digest": DIGEST});
    let decoded = decode_payload(kind, &bound);
    assert!(decoded.is_ok(), "{decoded:?}");
}

#[test]
fn capsules_carry_no_nested_userns_opt_out() {
    let mut config = merge_config(
        &SandboxConfig::default(),
        &StageSandboxConfig::default(),
        StageType::Standard,
        &Implementers::default(),
    );
    config.enabled = true;
    let json = generate_settings_json(&config);
    let nested = &json["sandbox"]["allowNestedUserNamespaces"];
    assert!(nested.is_null(), "got {nested}");
}
