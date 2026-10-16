//! Spawn-loop passes that must route a merge-state stage to human review
//! instead of holding the watch-mode daemon open, passes that must leave a
//! stage merged meanwhile untouched, and the orphan rule that retires a stale
//! merge writer with no verified PID identity.

use chrono::{Duration, Utc};

use super::super::merge_gate::stale_merge_retirement_blocks_spawn;
use super::super::resolver_attempts::{attempts_dir, attempts_file, MAX_MERGE_RESOLVER_ATTEMPTS};
use super::test_fixtures::{
    commit_on_stage_branch, git_ok, orchestrator_with_conflict, repo_with_clean_stage_branch,
    repo_with_conflicting_stage_branches, repo_with_stage_branches, use_real_worktree,
    with_read_only_stages,
};
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::recovery_guards::ORPHAN_PROBE_GRACE_SECS;
use crate::orchestrator::core::Orchestrator;
use crate::verify::transitions::{load_stage, save_stage, update_stage};

/// Run one spawn-loop pass, which must spawn nothing, record no resolver
/// attempt, and leave `stage_id` in `NeedsHumanReview`; returns its reason.
fn review_reason_after_one_pass(orchestrator: &mut Orchestrator, stage_id: &str) -> String {
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    let work_dir = &orchestrator.config.work_dir;
    assert!(!attempts_file(work_dir, stage_id).is_file());
    let stage = load_stage(stage_id, work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    stage
        .review_reason
        .expect("routing to human review records a reason")
}

/// Record `count` spent resolver attempts for `stage_id`.
fn spend_attempts(orchestrator: &Orchestrator, stage_id: &str, count: u32) {
    let work_dir = &orchestrator.config.work_dir;
    std::fs::create_dir_all(attempts_dir(work_dir)).unwrap();
    std::fs::write(attempts_file(work_dir, stage_id), count.to_string()).unwrap();
}

#[test]
fn a_missing_stage_branch_routes_the_stage_to_review() {
    let repo = repo_with_stage_branches(&[]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "gone");
    let reason = review_reason_after_one_pass(&mut orchestrator, "gone");
    assert!(reason.contains("branch loom/gone is missing"), "{reason}");
    assert!(reason.contains("--force-complete"), "{reason}");
}

#[test]
fn a_missing_target_branch_routes_the_stage_to_review() {
    let repo = repo_with_stage_branches(&["untargeted"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "untargeted");
    orchestrator.config.base_branch = Some("trunk".to_string());
    let reason = review_reason_after_one_pass(&mut orchestrator, "untargeted");
    assert!(
        reason.contains("target branch trunk is missing"),
        "{reason}"
    );
}

#[test]
fn a_spent_resolver_budget_escalates_the_stage_to_review() {
    let repo = repo_with_conflicting_stage_branches(&["spent"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "spent");
    spend_attempts(&orchestrator, "spent", MAX_MERGE_RESOLVER_ATTEMPTS);
    let reason = review_reason_after_one_pass(&mut orchestrator, "spent");
    let gave_up = format!("gave up after {MAX_MERGE_RESOLVER_ATTEMPTS} attempt(s)");
    assert!(reason.contains(&gave_up), "{reason}");
}

#[test]
fn an_attempt_the_counter_cannot_record_routes_the_stage_to_review() {
    let repo = repo_with_conflicting_stage_branches(&["unrecorded"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unrecorded");
    // A file where the counter directory belongs: the counter reads 0, but no
    // attempt can be written, so no resolver may spawn.
    std::fs::write(attempts_dir(&orchestrator.config.work_dir), "not a dir").unwrap();
    let reason = review_reason_after_one_pass(&mut orchestrator, "unrecorded");
    assert!(reason.contains("could not be recorded"), "{reason}");
}

#[test]
fn unreadable_merge_signals_route_the_stage_to_review() {
    let repo = repo_with_stage_branches(&["unknown"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unknown");
    // A directory named like a signal file cannot be read as one, and no
    // session record attributes it to a stage.
    let signals = orchestrator.config.work_dir.join("signals");
    std::fs::create_dir_all(signals.join("broken.md")).unwrap();
    let reason = review_reason_after_one_pass(&mut orchestrator, "unknown");
    assert!(
        reason.contains("Could not tell whether one is already running"),
        "{reason}"
    );
}

#[test]
fn a_review_routing_that_cannot_be_saved_is_retried_next_pass() {
    let repo = repo_with_stage_branches(&[]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unsaved");
    let first_pass = with_read_only_stages(&mut orchestrator, |orchestrator| {
        orchestrator.spawn_merge_resolution_sessions().unwrap()
    });
    let Some(spawned) = first_pass else {
        // Stage files stay writable for this process; nothing to simulate.
        return;
    };
    assert_eq!(spawned, 0);
    let stage = load_stage("unsaved", &orchestrator.config.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    let reason = review_reason_after_one_pass(&mut orchestrator, "unsaved");
    assert!(
        reason.contains("branch loom/unsaved is missing"),
        "{reason}"
    );
}

/// Each stage reaches a different decision of the pass: the missing-branch
/// routing, the merge gate, the budget escalation, and the reservation.
#[test]
fn a_stage_merged_while_the_pass_held_a_stale_copy_is_left_untouched() {
    let repo = repo_with_conflicting_stage_branches(&["gated", "spent", "due"]);
    commit_on_stage_branch(repo.path(), "gated", ".claude/settings.json");
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "gone");
    spend_attempts(&orchestrator, "spent", MAX_MERGE_RESOLVER_ATTEMPTS);
    let work_dir = orchestrator.config.work_dir.clone();
    for stage_id in ["gated", "spent", "due"] {
        let stage = Stage {
            id: stage_id.to_string(),
            status: StageStatus::MergeConflict,
            ..Stage::default()
        };
        save_stage(&stage, &work_dir).unwrap();
    }
    for stage_id in ["gone", "gated", "spent", "due"] {
        let stale = load_stage(stage_id, &work_dir).unwrap();
        // A concurrent `loom stage merge` finishes the merge.
        update_stage(stage_id, &work_dir, |stage| {
            stage.status = StageStatus::Completed;
            stage.merged = true;
            Ok(())
        })
        .unwrap();

        assert!(!orchestrator.spawn_resolver_if_due(&stale), "{stage_id}");
        let on_disk = load_stage(stage_id, &work_dir).unwrap();
        assert_eq!(on_disk.status, StageStatus::Completed, "{stage_id}");
        assert!(on_disk.merged, "{stage_id}");
        assert_eq!(on_disk.review_reason, None, "{stage_id}");
    }
    let spent = std::fs::read_to_string(attempts_file(&work_dir, "spent")).unwrap();
    assert_eq!(spent, MAX_MERGE_RESOLVER_ATTEMPTS.to_string());
    assert!(!attempts_file(&work_dir, "due").exists());
}

#[test]
fn a_conflict_that_became_clean_is_landed_without_a_resolver() {
    let (repo, head) = repo_with_clean_stage_branch("healed");
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "healed");
    use_real_worktree(repo.path(), "healed");
    update_stage("healed", &orchestrator.config.work_dir, |stage| {
        stage.completed_commit = Some(head);
        Ok(())
    })
    .unwrap();
    let work_dir = orchestrator.config.work_dir.clone();

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);

    let stage = load_stage("healed", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert!(!attempts_file(&work_dir, "healed").exists());
    assert!(orchestrator.active_sessions.is_empty());
    assert!(!repo.path().join(".worktrees").join("healed").exists());
}

#[test]
fn a_clean_merge_with_no_proof_of_the_recorded_commit_goes_to_review() {
    let (repo, _head) = repo_with_clean_stage_branch("unproven");
    git_ok(repo.path(), &["checkout", "-q", "--orphan", "side"]);
    git_ok(
        repo.path(),
        &["commit", "-q", "--allow-empty", "-m", "elsewhere"],
    );
    let elsewhere =
        crate::git::runner::run_git_checked(&["rev-parse", "HEAD"], repo.path()).unwrap();
    git_ok(repo.path(), &["checkout", "-q", "-f", "main"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unproven");
    update_stage("unproven", &orchestrator.config.work_dir, |stage| {
        stage.completed_commit = Some(elsewhere.clone());
        Ok(())
    })
    .unwrap();

    let reason = review_reason_after_one_pass(&mut orchestrator, "unproven");

    assert!(reason.contains("recorded commit is not in"), "{reason}");
    assert!(reason.contains("inspect loom/unproven"), "{reason}");
}

/// A stale tracked session of `session_type`, created `age_secs` ago.
fn stale_session(session_type: SessionType, age_secs: i64) -> Session {
    let mut session = Session::new();
    session.session_type = session_type;
    session.assign_to_stage("stale-writer".to_string());
    session.created_at = Utc::now() - Duration::seconds(age_secs);
    session
}

/// `stale_merge_retirement_blocks_spawn` for a session with no verified PID
/// identity whose kill fails and whose liveness probe answers `alive`.
fn identityless_blocks(session: &Session, alive: bool) -> bool {
    stale_merge_retirement_blocks_spawn(
        session,
        false,
        |_| Ok(alive),
        |_| Err(anyhow::anyhow!("no PID identity to signal")),
        |_| panic!("death is never confirmed by PID without a PID identity"),
    )
}

#[test]
fn an_identityless_writer_found_dead_past_the_orphan_grace_is_retired() {
    let old = ORPHAN_PROBE_GRACE_SECS + 5;
    for session_type in [SessionType::Stage, SessionType::Merge] {
        let session = stale_session(session_type, old);
        assert!(!identityless_blocks(&session, false), "{session_type:?}");
    }
}

#[test]
fn an_identityless_writer_keeps_ownership_while_young_or_alive() {
    let young = stale_session(SessionType::Stage, 0);
    assert!(identityless_blocks(&young, false));
    let old = stale_session(SessionType::Stage, ORPHAN_PROBE_GRACE_SECS + 5);
    assert!(identityless_blocks(&old, true));
}
