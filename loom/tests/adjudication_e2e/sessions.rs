//! Adjudication-session budget and escalation tests using the parent fixtures.

use super::*;
use loom::models::session::Session;
use loom::orchestrator::adjudication::MAX_ADJUDICATION_ATTEMPTS;

/// A session that died without recording anything must be replaced — but only
/// while the dispute's budget lasts, after which the stage asks for a human
/// instead of collecting adjudicators forever. The registry returns that
/// escalation; the daemon writes it once the disputing agent is retired.
#[test]
fn a_dead_session_is_replaced_until_the_budget_runs_out() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    write_stage(work, &make_stage("s1"));
    write_dispute(work, "s1", 1);

    let reg = AdjudicatorRegistry::new();
    for attempt in 1..=MAX_ADJUDICATION_ATTEMPTS {
        assert_eq!(
            reg.disputes_awaiting_session(work).unwrap().jobs.len(),
            1,
            "attempt {attempt} should still be offered a session",
        );
    }
    let pending = reg.disputes_awaiting_session(work).unwrap();
    assert!(
        pending.jobs.is_empty(),
        "the budget is spent; no further session may be started",
    );
    assert_eq!(pending.escalations.len(), 1);
    pending.escalations[0].write(work);

    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::NeedsHumanReview);
    assert!(!verdict_file(&work.join("disputes"), "s1", 1).exists());
}

/// The escalation closes the dispute it abandons: once the review is approved
/// and a fresh session files the next dispute, that one is judged, instead of
/// the stale one being escalated again at its spent budget.
#[test]
fn an_escalated_dispute_does_not_shadow_the_next_one() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    write_stage(work, &make_stage("s1"));
    write_dispute(work, "s1", 1);
    let reg = AdjudicatorRegistry::new();
    for _ in 0..MAX_ADJUDICATION_ATTEMPTS {
        reg.disputes_awaiting_session(work).unwrap();
    }
    let pending = reg.disputes_awaiting_session(work).unwrap();
    assert_eq!(pending.escalations.len(), 1);
    pending.escalations[0].write(work);

    // Approved, re-run, and disputed again by the fresh session.
    loom::verify::transitions::update_stage("s1", work, |stage| {
        stage.try_approve_review()?;
        stage.try_mark_executing()?;
        stage.try_request_adjudication(None)
    })
    .unwrap();
    write_dispute(work, "s1", 2);

    let pending = reg.disputes_awaiting_session(work).unwrap();
    assert!(pending.escalations.is_empty(), "{:?}", pending.escalations);
    let offered: Vec<u32> = pending.jobs.iter().map(|job| job.request.id).collect();
    assert_eq!(offered, vec![2]);
    assert_eq!(reg.unanswered_disputes(work, "s1").unwrap(), 1);
}

/// Two adjudicators judging the same stage in the same main repository is the
/// thing the daemon must never do, so a live session suppresses the next offer.
#[test]
fn a_live_adjudication_session_blocks_a_second_one() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_plan(work);
    write_stage(work, &make_stage("s1"));
    write_dispute(work, "s1", 1);

    // A session record plus PID-identity evidence for a process that really is
    // alive (this test's own). A PID file with no start-time line reads back as
    // unverifiable, which every liveness probe treats as alive.
    let session = Session::new_adjudication("s1");
    loom::fs::session_files::save_session(&session, work).unwrap();
    let pids = work.join("pids");
    std::fs::create_dir_all(&pids).unwrap();
    std::fs::write(
        pids.join(format!("{}-{}.pid", session.tracking_key, session.id)),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    let reg = AdjudicatorRegistry::new();
    let pending = reg.disputes_awaiting_session(work).unwrap();
    assert!(
        pending.jobs.is_empty() && pending.escalations.is_empty(),
        "a live adjudication session must suppress a second one",
    );
}
