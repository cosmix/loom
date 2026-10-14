//! A merge resolver giving up its stage's merge: who may, what it stops, and
//! what reaches the review reason and the ledger.

use std::path::PathBuf;

use super::super::resolver_attempts::{attempts_file, ReservedAttempt};
use super::super::resolver_spawn::test_fixtures::{
    orchestrator_with_conflict, repo_with_stage_branches, resolver_for, write_pid_file,
    write_signal,
};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::git::MergeBlock;
use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::models::stage::StageStatus;
use crate::orchestrator::core::Orchestrator;
use crate::verify::transitions::{load_stage, update_stage};

/// A running resolver for `stage_id` that `orchestrator` tracks. Its PID
/// identity proves it gone once killed: the start time never matches.
fn tracked_resolver(orchestrator: &mut Orchestrator, stage_id: &str) -> (Session, PathBuf) {
    let work_dir = orchestrator.config.work_dir.clone();
    let mut resolver = resolver_for(stage_id);
    resolver.status = SessionStatus::Running;
    save_session(&resolver, &work_dir).unwrap();
    write_pid_file(&work_dir, &resolver, Some(u64::MAX));
    let signal = write_signal(orchestrator, &resolver.id);
    ReservedAttempt::record(&work_dir, stage_id, 0)
        .unwrap()
        .keep();
    orchestrator
        .active_sessions
        .insert(stage_id.to_string(), resolver.clone());
    (resolver, signal)
}

#[test]
fn a_resolver_that_gives_up_is_stopped_and_its_reason_reaches_the_review() {
    let repo = repo_with_stage_branches(&["gave-up"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "gave-up");
    let work_dir = orchestrator.config.work_dir.clone();
    let (resolver, signal) = tracked_resolver(&mut orchestrator, "gave-up");

    let outcome = orchestrator
        .hold_merge_for_resolver("gave-up", &resolver, "doc/specs is read-only")
        .unwrap();

    assert!(outcome.contains("needs-human-review"), "{outcome}");
    assert!(outcome.contains("was stopped"), "{outcome}");
    let stage = load_stage("gave-up", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    let said = format!(
        "merge resolver {} could not finish the merge: doc/specs is read-only",
        resolver.id
    );
    assert!(reason.contains(&said), "{reason}");
    let stopped = format!("loom stopped merge resolver session {}", resolver.id);
    assert!(reason.contains(&stopped), "{reason}");
    assert!(!orchestrator.active_sessions.contains_key("gave-up"));
    assert!(!signal.exists());
    assert!(!attempts_file(&work_dir, "gave-up").exists());
    let record = load_session_exact(&work_dir, &resolver.id)
        .unwrap()
        .unwrap();
    assert_eq!(record.exit_reason, Some(SessionExitReason::OperatorStop));
}

#[test]
fn a_give_up_that_could_not_prove_the_stop_says_so() {
    let repo = repo_with_stage_branches(&["unproven"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unproven");
    let work_dir = orchestrator.config.work_dir.clone();
    let (resolver, _) = tracked_resolver(&mut orchestrator, "unproven");
    // Alive but unverifiable: no kill signals it, and nothing proves it gone.
    write_pid_file(&work_dir, &resolver, None);

    let outcome = orchestrator
        .hold_merge_for_resolver("unproven", &resolver, "cannot merge")
        .unwrap();

    assert!(outcome.contains("could not confirm"), "{outcome}");
    assert!(!outcome.contains("was stopped"), "{outcome}");
    let reason = load_stage("unproven", &work_dir)
        .unwrap()
        .review_reason
        .unwrap();
    assert!(reason.contains("may still be running"), "{reason}");
}

#[test]
fn a_give_up_flattens_the_text_the_agent_wrote() {
    let repo = repo_with_stage_branches(&["hostile"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "hostile");
    let (resolver, _) = tracked_resolver(&mut orchestrator, "hostile");

    orchestrator
        .hold_merge_for_resolver("hostile", &resolver, "line one\n\u{1b}[2Jline two")
        .unwrap();

    let reason = load_stage("hostile", &orchestrator.config.work_dir)
        .unwrap()
        .review_reason
        .unwrap();
    assert!(!reason.contains('\u{1b}'), "{reason:?}");
    assert!(reason.contains("line one"), "{reason:?}");
    assert!(reason.contains("line two"), "{reason:?}");
}

#[test]
fn a_retired_resolver_cannot_give_up_the_merge_its_successor_is_resolving() {
    let repo = repo_with_stage_branches(&["handover"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "handover");
    let work_dir = orchestrator.config.work_dir.clone();
    let mut retired = resolver_for("handover");
    retired.status = SessionStatus::Completed;
    save_session(&retired, &work_dir).unwrap();
    let (successor, signal) = tracked_resolver(&mut orchestrator, "handover");

    let refusal = orchestrator
        .hold_merge_for_resolver("handover", &retired, "cannot merge")
        .unwrap_err();

    assert!(refusal.contains("no longer running"), "{refusal}");
    let stage = load_stage("handover", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(orchestrator.active_sessions["handover"].id, successor.id);
    assert!(signal.exists());
    assert!(attempts_file(&work_dir, "handover").exists());
}

#[test]
fn a_running_session_that_is_not_the_stages_resolver_cannot_give_up() {
    let repo = repo_with_stage_branches(&["stranger"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "stranger");
    let work_dir = orchestrator.config.work_dir.clone();
    let (tracked, _) = tracked_resolver(&mut orchestrator, "stranger");
    let mut stranger = resolver_for("stranger");
    stranger.status = SessionStatus::Running;
    save_session(&stranger, &work_dir).unwrap();

    let refusal = orchestrator
        .hold_merge_for_resolver("stranger", &stranger, "cannot merge")
        .unwrap_err();

    assert!(refusal.contains("not the live merge resolver"), "{refusal}");
    let stage = load_stage("stranger", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(orchestrator.active_sessions["stranger"].id, tracked.id);
}

#[test]
fn a_give_up_is_not_repeated() {
    let repo = repo_with_stage_branches(&["once"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "once");
    let (resolver, _) = tracked_resolver(&mut orchestrator, "once");
    orchestrator
        .hold_merge_for_resolver("once", &resolver, "cannot merge")
        .unwrap();

    let refusal = orchestrator
        .hold_merge_for_resolver("once", &resolver, "cannot merge")
        .unwrap_err();

    assert!(
        refusal.contains("not MergeConflict or MergeBlocked"),
        "{refusal}"
    );
}

#[test]
fn a_give_up_does_not_cancel_the_retry_of_an_accepted_resolution() {
    let repo = repo_with_stage_branches(&["retried"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "retried");
    let work_dir = orchestrator.config.work_dir.clone();
    let (resolver, _) = tracked_resolver(&mut orchestrator, "retried");
    update_stage("retried", &work_dir, |stage| {
        stage.block_merge(MergeBlock::TargetMoved);
        Ok(())
    })
    .unwrap();

    let refusal = orchestrator
        .hold_merge_for_resolver("retried", &resolver, "cannot merge")
        .unwrap_err();

    assert!(
        refusal.contains("loom retries the merge itself"),
        "{refusal}"
    );
    let stage = load_stage("retried", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    assert_eq!(orchestrator.active_sessions["retried"].id, resolver.id);
}
