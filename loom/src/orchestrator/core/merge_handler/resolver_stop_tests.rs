//! Settling a resolver spawn's outcome, and stopping the resolver of a branch
//! the merge gate holds. No backend spawn runs here: the harness has no stub
//! backend, so the backend's answer is handed to `settle_resolver_spawn`, and
//! a lane the harness cannot build is settled from the evidence its teardown
//! would observe.

use super::super::resolver_attempts::{attempts_file, merge_resolver_attempts, ReservedAttempt};
use super::super::resolver_spawn::test_fixtures::{
    commit_on_stage_branch, orchestrator_with_conflict, repo_with_stage_branches, resolver_for,
    with_read_only_stages, write_pid_file, write_signal,
};
use super::{teardown_proves_gone, UnstoppedResolver};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::models::session::{Session, SessionBackendKind, SessionExitReason, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::signals::generate_merge_signal;
use crate::orchestrator::terminal::native::{
    pid_only_is_alive, pid_only_terminate, write_test_pid_identity,
};
use crate::verify::transitions::load_stage;

#[test]
fn a_spawned_resolver_keeps_its_attempt() {
    let repo = repo_with_stage_branches(&["kept"]);
    let orchestrator = orchestrator_with_conflict(repo.path(), "kept");
    let work_dir = orchestrator.config.work_dir.clone();
    let attempt = ReservedAttempt::record(&work_dir, "kept", 0).unwrap();
    let session = resolver_for("kept");
    let spawned = Ok(session.clone());
    let settled = orchestrator.settle_resolver_spawn("kept", session.clone(), spawned, attempt);
    assert_eq!(settled.unwrap().id, session.id);
    assert_eq!(merge_resolver_attempts(&work_dir, "kept"), 1);
}

#[test]
fn a_failed_spawn_that_left_nothing_gives_its_attempt_back() {
    let repo = repo_with_stage_branches(&["refunded"]);
    let orchestrator = orchestrator_with_conflict(repo.path(), "refunded");
    let work_dir = orchestrator.config.work_dir.clone();
    let attempt = ReservedAttempt::record(&work_dir, "refunded", 0).unwrap();
    let session = Session::new_merge("loom/refunded".to_string(), "main".to_string());
    let signal = write_signal(&orchestrator, &session.id);
    let failed = Err(anyhow::anyhow!("tmux new-session failed"));
    let error = orchestrator
        .settle_resolver_spawn("refunded", session, failed, attempt)
        .unwrap_err();
    assert!(
        error.downcast_ref::<UnstoppedResolver>().is_none(),
        "{error:#}"
    );
    assert!(!attempts_file(&work_dir, "refunded").exists());
    assert!(!signal.exists());
}

#[test]
fn a_failed_spawn_that_may_have_left_a_resolver_keeps_its_attempt() {
    let repo = repo_with_stage_branches(&["unstopped"]);
    let orchestrator = orchestrator_with_conflict(repo.path(), "unstopped");
    let work_dir = orchestrator.config.work_dir.clone();
    let attempt = ReservedAttempt::record(&work_dir, "unstopped", 0).unwrap();
    let session = resolver_for("unstopped");
    write_pid_file(&work_dir, &session, None);
    let signal = write_signal(&orchestrator, &session.id);
    let failed = Err(anyhow::anyhow!("tmux has-session failed"));
    let error = orchestrator
        .settle_resolver_spawn("unstopped", session.clone(), failed, attempt)
        .unwrap_err();
    let unstopped = error.downcast_ref::<UnstoppedResolver>().unwrap();
    assert_eq!(unstopped.session.id, session.id);
    assert_eq!(merge_resolver_attempts(&work_dir, "unstopped"), 1);
    assert!(!signal.exists());
}

#[test]
fn a_failed_spawn_on_a_native_lane_with_no_terminal_is_retried_with_its_attempt_back() {
    let repo = repo_with_stage_branches(&["headless"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "headless");
    let work_dir = orchestrator.config.work_dir.clone();
    let attempt = ReservedAttempt::record(&work_dir, "headless", 0).unwrap();
    let mut session = resolver_for("headless");
    session.backend = SessionBackendKind::Native;
    let signal = write_signal(&orchestrator, &session.id);
    // With no terminal, `SessionBackend` kills and probes a native session
    // through the PID layers alone, and its spawn launched nothing.
    let killed = pid_only_terminate(&work_dir, &session).is_ok();
    assert!(!killed, "the kill refuses for want of a PID identity");
    let probe = || !pid_only_is_alive(&work_dir, &session);
    let gone = teardown_proves_gone(SessionBackendKind::Native, killed, probe);
    let failed = anyhow::anyhow!("No terminal emulator found");
    let error = orchestrator
        .settle_failed_spawn(session, failed, attempt, gone)
        .unwrap_err();
    assert!(
        error.downcast_ref::<UnstoppedResolver>().is_none(),
        "{error:#}"
    );
    assert!(!attempts_file(&work_dir, "headless").exists());
    assert!(!signal.exists());
    orchestrator.report_merge_spawn_failure("headless", error);
    let stage = load_stage("headless", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(stage.review_reason, None);
}

#[test]
fn a_native_teardown_is_judged_by_the_probe_alone() {
    let native = SessionBackendKind::Native;
    for killed in [true, false] {
        assert!(teardown_proves_gone(native, killed, || true));
        assert!(!teardown_proves_gone(native, killed, || false));
    }
}

#[test]
fn a_tmux_teardown_needs_a_successful_kill_and_the_probe() {
    let tmux = SessionBackendKind::Tmux;
    let unasked = || -> bool { panic!("a failed tmux kill decides without the probe") };
    assert!(!teardown_proves_gone(tmux, false, unasked));
    assert!(teardown_proves_gone(tmux, true, || true));
    assert!(!teardown_proves_gone(tmux, true, || false));
}

#[test]
fn a_spawned_resolver_with_no_session_record_holds_its_stage_while_tracked() {
    let repo = repo_with_stage_branches(&["unrecorded"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unrecorded");
    let work_dir = orchestrator.config.work_dir.clone();
    let mut resolver = resolver_for("unrecorded");
    resolver.backend = SessionBackendKind::Tmux;
    resolver.status = SessionStatus::Running;
    // The spawn recorded a verified PID identity; its session record was
    // never saved, so its signal reads as stale.
    write_test_pid_identity(&work_dir, &resolver, std::process::id()).unwrap();
    let stage = Stage {
        id: "unrecorded".to_string(),
        ..Stage::default()
    };
    generate_merge_signal(&resolver, &stage, "loom/unrecorded", "main", &[], &work_dir).unwrap();
    let tracked = resolver.clone();
    orchestrator
        .active_sessions
        .insert("unrecorded".to_string(), tracked);

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(orchestrator.active_sessions["unrecorded"].id, resolver.id);
    assert!(!attempts_file(&work_dir, "unrecorded").exists());
    let stage = load_stage("unrecorded", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
}

/// The error `settle_resolver_spawn` returns for a resolver it could not stop.
fn unstopped_error(stage_id: &str) -> (Session, anyhow::Error) {
    let session = resolver_for(stage_id);
    let unstopped = UnstoppedResolver {
        session: session.clone(),
    };
    let error = anyhow::anyhow!("tmux has-session failed").context(unstopped);
    (session, error)
}

#[test]
fn a_resolver_that_may_still_run_routes_the_stage_to_review() {
    let repo = repo_with_stage_branches(&["unstopped"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unstopped");
    let (session, error) = unstopped_error("unstopped");
    orchestrator.report_merge_spawn_failure("unstopped", error);
    let stage = load_stage("unstopped", &orchestrator.config.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    assert!(reason.contains(&session.id), "{reason}");
    assert!(reason.contains("may still be running"), "{reason}");
    assert!(reason.contains("git merge --abort"), "{reason}");
    assert!(
        reason.contains("Spawn error: tmux has-session failed"),
        "{reason}"
    );
    assert!(!orchestrator.active_sessions.contains_key("unstopped"));
}

#[test]
fn a_resolver_that_may_still_run_is_tracked_when_its_review_cannot_be_saved() {
    let repo = repo_with_stage_branches(&["unsaved"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "unsaved");
    let (session, error) = unstopped_error("unsaved");
    let routed = with_read_only_stages(&mut orchestrator, |orchestrator| {
        orchestrator.report_merge_spawn_failure("unsaved", error);
    });
    if routed.is_none() {
        // Stage files stay writable for this process; nothing to simulate.
        return;
    }
    let stage = load_stage("unsaved", &orchestrator.config.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(orchestrator.active_sessions["unsaved"].id, session.id);
}

#[test]
fn a_gated_branch_has_its_tracked_resolver_stopped_before_review() {
    let repo = repo_with_stage_branches(&["gated"]);
    commit_on_stage_branch(repo.path(), "gated", ".mcp.json");
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "gated");
    let work_dir = orchestrator.config.work_dir.clone();
    let mut resolver = resolver_for("gated");
    resolver.status = SessionStatus::Running;
    save_session(&resolver, &work_dir).unwrap();
    // A mismatched start time: the kill signals nothing, and the PID
    // identity proves the resolver gone.
    write_pid_file(&work_dir, &resolver, Some(u64::MAX));
    let signal = write_signal(&orchestrator, &resolver.id);
    let tracked = resolver.clone();
    orchestrator
        .active_sessions
        .insert("gated".to_string(), tracked);

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    let stage = load_stage("gated", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    assert!(reason.contains("touches control path(s)"), "{reason}");
    let stopped = format!("loom stopped merge resolver session {}", resolver.id);
    assert!(reason.contains(&stopped), "{reason}");
    assert!(reason.contains("git merge --abort"), "{reason}");
    assert!(!orchestrator.active_sessions.contains_key("gated"));
    assert!(!signal.exists());
    let record = load_session_exact(&work_dir, &resolver.id)
        .unwrap()
        .unwrap();
    assert_eq!(record.exit_reason, Some(SessionExitReason::OperatorStop));
}

#[test]
fn a_missing_stage_worktree_gives_the_attempt_back_and_removes_the_signal() {
    let repo = repo_with_stage_branches(&["no-worktree"]);
    let orchestrator = orchestrator_with_conflict(repo.path(), "no-worktree");
    let work_dir = orchestrator.config.work_dir.clone();
    std::fs::remove_dir(repo.path().join(".worktrees").join("no-worktree")).unwrap();
    let attempt = ReservedAttempt::record(&work_dir, "no-worktree", 0).unwrap();
    let session = resolver_for("no-worktree");
    let signal = write_signal(&orchestrator, &session.id);
    let stage = Stage {
        id: "no-worktree".to_string(),
        ..Stage::default()
    };
    let error = orchestrator
        .launch_resolver(&stage, session, &signal, attempt)
        .unwrap_err();
    assert!(format!("{error:#}").contains("worktree"), "{error:#}");
    assert!(!attempts_file(&work_dir, "no-worktree").exists());
    assert!(!signal.exists());
}

#[test]
fn a_missing_stage_worktree_routes_the_stage_to_review() {
    let repo = repo_with_stage_branches(&["no-worktree"]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), "no-worktree");
    std::fs::remove_dir(repo.path().join(".worktrees").join("no-worktree")).unwrap();
    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    let work_dir = &orchestrator.config.work_dir;
    let stage = load_stage("no-worktree", work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    let reason = stage.review_reason.unwrap();
    assert!(
        reason.contains(".worktrees/no-worktree is missing"),
        "{reason}"
    );
    assert!(!attempts_file(work_dir, "no-worktree").exists());
}
