//! The relayed `check` handler: who may queue a check, and what it refuses.

use chrono::Utc;
use serde_json::{json, Value};

use crate::fs::inbox::write_entry;
use crate::host_verify::store::load_current;
use crate::host_verify::JobState;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::relay::{AgentRole, RequestKind};
use crate::test_git::{git, TestRepo};

use super::super::test_support::{entry_for, fixture, Fixture, STAGE};
use super::super::{run_pass, Settle};

/// A repository at the fixture's repo root with `s1`'s registered
/// worktree; returns the worktree's HEAD.
pub(super) fn repository(fx: &Fixture) -> String {
    TestRepo::builder()
        .identity("t", "t@t")
        .no_seed()
        .init_in(&fx.repo_root);
    crate::git::worktree::capture_main_identity(&fx.repo_root, &fx.work_dir).unwrap();
    git(
        &fx.repo_root,
        &["commit", "-q", "--allow-empty", "-m", "base"],
    );
    let add = ["worktree", "add", "-q", ".worktrees/s1", "-b", "loom/s1"];
    git(&fx.repo_root, &add);
    let worktree = fx.repo_root.join(".worktrees").join(STAGE);
    git(&worktree, &["rev-parse", "HEAD"])
}

pub(super) fn request(head: &str) -> Value {
    json!({"head": head, "tree_digest": "0".repeat(64), "no_cache": false})
}

/// Relay `payload` as `agent` and return how the drain settled it.
pub(super) fn settle(
    fx: &Fixture,
    record: &Session,
    agent: AgentRole,
    payload: Value,
    alive: bool,
) -> Settle {
    let mut entry = entry_for(record, RequestKind::Check, payload);
    entry.agent = agent;
    write_entry(&fx.work_dir, &entry).unwrap();
    let mut host = fx.host(alive);
    let report = run_pass(&mut host, &fx.tick(Utc::now()));
    let found = report.settled.into_iter().find(|(id, _)| *id == entry.id);
    found.unwrap().1
}

pub(super) fn refused(settled: Settle) -> String {
    match settled {
        Settle::Refused(reason) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

pub(super) fn queued_check(fx: &Fixture) -> bool {
    load_current(&fx.work_dir, STAGE)
        .unwrap()
        .is_some_and(|job| job.state == JobState::Queued)
}

#[test]
fn an_owning_stage_session_queues_one_check_job() {
    let fx = fixture();
    let head = repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    let settled = settle(&fx, &record, AgentRole::Main, request(&head), true);
    assert!(matches!(&settled, Settle::Applied(Some(note)) if note.starts_with("host job ")));
    assert!(queued_check(&fx));
}

#[test]
fn a_subagent_of_the_owning_session_is_queued_too() {
    let fx = fixture();
    let head = repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    let settled = settle(&fx, &record, AgentRole::Subagent, request(&head), true);
    assert!(matches!(settled, Settle::Applied(Some(_))), "{settled:?}");
    assert!(queued_check(&fx));
}

#[test]
fn a_check_waiting_behind_another_settles_with_the_waiting_job_id() {
    use crate::host_verify::submit::{submit, Submission, SubmitRequest};
    let tmp = tempfile::tempdir().unwrap();
    let request = SubmitRequest {
        stage_id: "s1",
        session_id: "session-a",
        request_id: "r1",
        kind: crate::host_verify::JobKind::Check,
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tree_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        no_cache: false,
        criteria: Some(vec![1]),
    };
    submit(tmp.path(), &request, Utc::now()).unwrap();
    let request = SubmitRequest {
        request_id: "r2",
        criteria: Some(vec![2]),
        ..request
    };
    let waiting = submit(tmp.path(), &request, Utc::now()).unwrap();
    let expected = match &waiting {
        Submission::Waiting { pending, behind } => {
            format!("host job {} queued behind {behind}", pending.id)
        }
        other => panic!("expected a waiting job, got {other:?}"),
    };
    assert!(
        matches!(super::settle_submission(Ok(waiting)), Settle::Applied(Some(note)) if note == expected)
    );
}

#[test]
fn a_live_judge_of_the_stage_is_queued() {
    let fx = fixture();
    let head = repository(&fx);
    let judge = fx.record(SessionType::Adjudication, SessionStatus::Running);
    fx.stage(StageStatus::NeedsAdjudication, None);
    let settled = settle(&fx, &judge, AgentRole::Main, request(&head), true);
    assert!(matches!(settled, Settle::Applied(Some(_))), "{settled:?}");
    assert!(queued_check(&fx));
    // Only a merge resolver forces a fresh run; a judge keeps the request's policy.
    let job = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert!(!job.no_cache);
}

#[test]
fn a_session_that_does_not_own_the_stage_is_refused() {
    let fx = fixture();
    let head = repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some("someone-else"));
    let reason = refused(settle(&fx, &record, AgentRole::Main, request(&head), true));
    assert!(reason.contains("not the active session"), "{reason}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_judge_that_is_not_alive_is_refused() {
    let fx = fixture();
    let head = repository(&fx);
    let judge = fx.record(SessionType::Adjudication, SessionStatus::Running);
    fx.stage(StageStatus::NeedsAdjudication, None);
    let reason = refused(settle(&fx, &judge, AgentRole::Main, request(&head), false));
    assert!(reason.contains("live adjudication session"), "{reason}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_judge_of_another_stage_is_refused() {
    let fx = fixture();
    let head = repository(&fx);
    let mut judge = Session::new();
    judge.session_type = SessionType::Adjudication;
    judge.assign_to_stage("other".to_string());
    judge.status = SessionStatus::Running;
    crate::fs::session_files::save_session(&judge, &fx.work_dir).unwrap();
    fx.stage(StageStatus::NeedsAdjudication, None);
    let settled = settle(&fx, &judge, AgentRole::Main, request(&head), true);
    assert!(matches!(settled, Settle::Refused(_)), "{settled:?}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_malformed_head_or_digest_is_refused() {
    let fx = fixture();
    repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    let bad_head = refused(settle(&fx, &record, AgentRole::Main, request("ABC"), true));
    assert!(bad_head.contains("head must be"), "{bad_head}");
    let head = "a".repeat(40);
    let payload = json!({"head": head, "tree_digest": "xyz", "no_cache": false});
    let bad_digest = refused(settle(&fx, &record, AgentRole::Main, payload, true));
    assert!(bad_digest.contains("tree_digest must be"), "{bad_digest}");
}

#[test]
fn a_criterion_past_the_stage_list_is_refused_and_queues_nothing() {
    let fx = fixture();
    let head = repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    // The fixture stage has one acceptance criterion, so index 1 is past it.
    let mut payload = request(&head);
    payload["criteria"] = json!([1]);
    let reason = refused(settle(&fx, &record, AgentRole::Main, payload, true));
    assert!(reason.contains("out of range"), "{reason}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_head_that_is_not_the_checkouts_is_refused() {
    let fx = fixture();
    repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    let wrong = "a".repeat(40);
    let reason = refused(settle(&fx, &record, AgentRole::Main, request(&wrong), true));
    assert!(reason.contains("run loom check again"), "{reason}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_submission_that_fails_settles_refused() {
    let fx = fixture();
    let head = repository(&fx);
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    let elsewhere = fx.work_dir.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, fx.work_dir.join("verification")).unwrap();
    let reason = refused(settle(&fx, &record, AgentRole::Main, request(&head), true));
    assert!(reason.contains("could not be applied"), "{reason}");
}
