//! A relayed `check` from a merge resolver: queued only while the session
//! resolves this stage's merge, and worth nothing as landing evidence.

use chrono::Utc;

use crate::fs::session_files::save_session;
use crate::host_verify::apply::{apply_finished, Applied};
use crate::host_verify::store::{load_current, save_current};
use crate::host_verify::{JobKind, JobState};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::relay::AgentRole;
use crate::verify::transitions::load_stage;

use super::super::test_support::{fixture, Fixture, STAGE};
use super::super::Settle;
use super::tests::{queued_check, refused, repository, request, settle};

/// A merge session in `status`, spawned to resolve the merge of `branch`.
fn resolver(fx: &Fixture, status: SessionStatus, branch: &str) -> Session {
    let mut record = fx.record(SessionType::Merge, status);
    record.merge_source_branch = Some(branch.to_string());
    save_session(&record, &fx.work_dir).unwrap();
    record
}

fn relay_check(fx: &Fixture, record: &Session, head: &str) -> Settle {
    settle(fx, record, AgentRole::Main, request(head), true)
}

#[test]
fn the_resolver_of_this_stage_is_queued_in_either_merge_status() {
    for status in [StageStatus::MergeConflict, StageStatus::MergeBlocked] {
        let fx = fixture();
        let head = repository(&fx);
        let record = resolver(&fx, SessionStatus::Running, "loom/s1");
        fx.stage(status, None);

        let settled = relay_check(&fx, &record, &head);

        assert!(matches!(settled, Settle::Applied(Some(_))), "{settled:?}");
        let job = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
        assert_eq!(job.kind, JobKind::Check);
        assert_eq!(job.state, JobState::Queued);
        assert_eq!(job.session_id, record.id);
    }
}

#[test]
fn the_resolver_of_another_stage_is_refused() {
    let fx = fixture();
    let head = repository(&fx);
    let record = resolver(&fx, SessionStatus::Running, "loom/other");
    fx.stage(StageStatus::MergeConflict, None);

    let reason = refused(relay_check(&fx, &record, &head));

    assert!(reason.contains("not resolving the merge"), "{reason}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_finished_resolver_is_refused() {
    let fx = fixture();
    let head = repository(&fx);
    let record = resolver(&fx, SessionStatus::Completed, "loom/s1");
    fx.stage(StageStatus::MergeConflict, None);

    let reason = refused(relay_check(&fx, &record, &head));

    assert!(reason.contains("no longer running"), "{reason}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_stage_outside_merge_resolution_is_refused() {
    for status in [StageStatus::Executing, StageStatus::Completed] {
        let fx = fixture();
        let head = repository(&fx);
        let record = resolver(&fx, SessionStatus::Running, "loom/s1");
        fx.stage(status, None);

        let reason = refused(relay_check(&fx, &record, &head));

        assert!(
            reason.contains("not MergeConflict or MergeBlocked"),
            "{reason}"
        );
        assert!(!queued_check(&fx));
    }
}

#[test]
fn a_resolver_recorded_as_the_stage_session_gets_no_owner_shortcut() {
    let fx = fixture();
    let head = repository(&fx);
    let record = fx.record(SessionType::Merge, SessionStatus::Running);
    fx.stage(StageStatus::MergeConflict, Some(&record.id));

    let reason = refused(relay_check(&fx, &record, &head));

    assert!(reason.contains("not resolving the merge"), "{reason}");
    assert!(!queued_check(&fx));
}

#[test]
fn a_passed_check_of_a_merge_changes_no_stage_state() {
    let fx = fixture();
    let head = repository(&fx);
    let record = resolver(&fx, SessionStatus::Running, "loom/s1");
    fx.stage(StageStatus::MergeConflict, None);
    relay_check(&fx, &record, &head);
    let mut job = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    job.state = JobState::Passed;
    job.finished_at = Some(Utc::now());
    save_current(&fx.work_dir, &job).unwrap();

    let applied = apply_finished(&fx.work_dir, &job).unwrap();

    assert!(matches!(applied, Applied::Ignored(_)), "{applied:?}");
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(!stage.merged);
}

#[test]
fn a_resolver_supersedes_a_cached_check_of_another_session_instead_of_joining_it() {
    use crate::host_verify::submit::{submit, Submission, SubmitRequest};

    let fx = fixture();
    let head = repository(&fx);
    let record = resolver(&fx, SessionStatus::Running, "loom/s1");
    fx.stage(StageStatus::MergeConflict, None);
    let tree = "0".repeat(64);
    let ordinary = SubmitRequest {
        stage_id: STAGE,
        session_id: "stage-session",
        request_id: "r1",
        kind: JobKind::Check,
        head: &head,
        tree_digest: &tree,
        no_cache: false,
        criteria: None,
    };
    let Submission::Queued(cached) = submit(&fx.work_dir, &ordinary, Utc::now()).unwrap() else {
        panic!("expected the ordinary check to queue");
    };

    let settled = relay_check(&fx, &record, &head);

    assert!(matches!(settled, Settle::Applied(Some(_))), "{settled:?}");
    let job = load_current(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_ne!(job.id, cached.id);
    assert_eq!(
        (job.session_id.as_str(), job.no_cache),
        (record.id.as_str(), true)
    );
}
