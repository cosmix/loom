//! Closing the disputes a stage abandons when it escalates to a human: the
//! closed ones never shadow the dispute a fresh session files after approval,
//! and every reader of dispute state treats them as settled.

use super::closed_disputes::{close_open_disputes, is_closed};
use super::record::record_verdict_text;
use super::tests::{make_stage, reject_verdict, write_dispute_request, write_stage, write_verdict};
use super::tests_verdicts::arrange_unnormalisable_accept_verdict;
use super::{AdjudicatorRegistry, Escalation, MAX_APPLY_ATTEMPTS};
use crate::models::dispute::{applied_marker, dispute_dir};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, update_stage};
use std::path::Path;

fn closed(work: &Path, dispute_id: u32) -> bool {
    is_closed(&dispute_dir(&work.join("disputes"), "s1", dispute_id))
}

/// Approve the review, re-run the stage, and file `dispute_id` from the fresh
/// session.
fn approve_and_dispute_again(work: &Path, dispute_id: u32) {
    update_stage("s1", work, |stage| {
        stage.try_approve_review()?;
        stage.try_mark_executing()?;
        stage.try_request_adjudication(None)
    })
    .unwrap();
    write_dispute_request(work, "s1", dispute_id, 0);
}

/// An apply-cap escalation marks its own verdict handled and closes siblings.
/// After approval the next dispute is the one offered a session, and the
/// capped verdict is never applied again, even had its best-effort
/// `applied.marker` failed to land.
#[test]
fn an_apply_cap_escalation_closes_its_verdict_and_every_sibling() {
    let (tmp, reg, applied) = arrange_unnormalisable_accept_verdict();
    let work = tmp.path();
    write_dispute_request(work, "s1", 2, 0);
    for _ in 0..MAX_APPLY_ATTEMPTS {
        reg.apply_pending_verdicts(work).unwrap();
    }
    let escalated = load_stage("s1", work).unwrap();
    assert_eq!(escalated.status, StageStatus::NeedsHumanReview);
    assert!(!closed(work, 1) && closed(work, 2));
    assert!(applied.exists());

    std::fs::remove_file(&applied).unwrap();
    approve_and_dispute_again(work, 3);

    assert_eq!(reg.pending_verdicts(work).unwrap(), vec![("s1".into(), 1)]);
    reg.apply_pending_verdicts(work).unwrap();
    assert!(
        applied.exists(),
        "the cap receipt restores a missing marker"
    );
    assert!(reg.pending_verdicts(work).unwrap().is_empty());
    let after = load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsAdjudication);
    let pending = reg.disputes_awaiting_session(work).unwrap();
    let offered: Vec<u32> = pending.jobs.iter().map(|job| job.request.id).collect();
    assert_eq!(offered, vec![3]);
    assert!(pending.escalations.is_empty());
}

/// A Reject verdict escalates the stage and closes the sibling it left
/// unanswered. After approval, the next pass offers a job for the new dispute
/// only, never for the stale sibling.
#[test]
fn a_reject_verdict_closes_an_unanswered_sibling() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    write_dispute_request(work, "s1", 2, 0);
    write_verdict(work, "s1", 1, reject_verdict(), 1);
    let reg = AdjudicatorRegistry::new();

    reg.apply_pending_verdicts(work).unwrap();

    assert_eq!(
        load_stage("s1", work).unwrap().status,
        StageStatus::NeedsHumanReview
    );
    assert!(!closed(work, 1));
    assert!(closed(work, 2));

    approve_and_dispute_again(work, 3);
    let pending = reg.disputes_awaiting_session(work).unwrap();
    let offered: Vec<u32> = pending.jobs.iter().map(|job| job.request.id).collect();
    assert_eq!(offered, vec![3]);
    assert!(pending.escalations.is_empty());
}

/// A judge still running when its dispute was closed cannot revive it once
/// the stage is disputed again; the new dispute takes its verdict as usual.
#[test]
fn a_closed_dispute_takes_no_verdict() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    std::fs::create_dir_all(work.join("stages")).unwrap();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    Escalation::evidence_cap("s1").write(work);
    approve_and_dispute_again(work, 2);
    let verdict = r#"{"verdict": "reject", "reasoning": "r",
        "citations": [{"file": "f", "excerpt": "e", "claim": "c"}]}"#;

    let error = record_verdict_text(work, "s1", 1, verdict, None).unwrap_err();
    assert!(format!("{error:#}").contains("was closed"), "{error:#}");
    record_verdict_text(work, "s1", 2, verdict, None).unwrap();
}

/// Only a dispute with no applied outcome is closed: an applied one is
/// already settled and keeps its record as it is.
#[test]
fn closing_leaves_an_applied_dispute_as_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    write_dispute_request(work, "s1", 1, 0);
    write_dispute_request(work, "s1", 2, 0);
    std::fs::write(applied_marker(&work.join("disputes"), "s1", 1), b"").unwrap();

    close_open_disputes(work, "s1");

    assert!(!closed(work, 1));
    assert!(closed(work, 2));
}

/// `loom stage reset` closes the disputes of every stage it resets, disputed or
/// not: a stage that never filed one must not gain a `disputes/<stage>`
/// directory (and its `.lock`) from the call.
#[test]
fn closing_a_never_disputed_stage_leaves_no_directory_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_dispute_request(work, "s1", 1, 0);

    close_open_disputes(work, "never-disputed");

    assert!(!work.join("disputes/never-disputed").exists());
    assert!(!closed(work, 1), "another stage's dispute is untouched");
}
