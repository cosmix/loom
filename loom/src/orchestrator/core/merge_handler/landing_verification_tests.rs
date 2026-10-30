use super::*;

#[test]
fn the_resolver_exit_hands_failed_verification_back_to_a_stage_session() {
    let (repo, mut orchestrator) = worktree_stage();
    let original = enter_conflict(&orchestrator, &repo);
    resolve_in_worktree(&repo);
    let before = main_tip(&repo);
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.acceptance = vec![crate::models::stage::AcceptanceCriterion::Simple(
            "false".into(),
        )];
        Ok(())
    })
    .unwrap();

    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit.as_deref(), Some(original.as_str()));
    assert_eq!(main_tip(&repo), before);
    assert!(worktree(&repo).exists());
    let refusal = stage.merge.handback_refusal.unwrap();
    assert!(refusal.contains("Acceptance criteria failed"), "{refusal}");
}

#[test]
fn failed_verification_without_a_merge_commit_is_not_handed_back() {
    let (repo, mut orchestrator) = worktree_stage();
    enter_conflict(&orchestrator, &repo);
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.acceptance = vec![crate::models::stage::AcceptanceCriterion::Simple(
            "false".into(),
        )];
        Ok(())
    })
    .unwrap();

    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(stage.merge.handbacks, 0);
    assert_eq!(stage.merge.handback_refusal, None);
}

#[test]
fn a_v2_resolved_merge_with_no_matching_review_is_handed_back() {
    let (repo, mut orchestrator) = worktree_stage();
    enter_conflict(&orchestrator, &repo);
    resolve_in_worktree(&repo);
    // A free lock file proves no daemon runs, so the fingerprint the review
    // gate needs is computed in this process instead of asked of a daemon.
    std::fs::write(orchestrator.config.work_dir.join("orchestrator.lock"), "").unwrap();
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.plan_version = 2;
        stage.stage_type = crate::models::stage::StageType::Standard;
        Ok(())
    })
    .unwrap();

    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();

    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Queued);
    let refusal = stage.merge.handback_refusal.unwrap();
    assert!(refusal.contains("no well-formed review round"), "{refusal}");
}

#[test]
fn a_late_resolver_exit_keeps_the_tracked_stage_session() {
    let (_repo, mut orchestrator) = worktree_stage();
    set_status(&orchestrator, StageStatus::Queued);
    let mut worker = Session::new();
    worker.session_type = crate::models::session::SessionType::Stage;
    let worker_id = worker.id.clone();
    orchestrator.active_sessions.insert(ID.to_string(), worker);

    orchestrator
        .handle_merge_session_completed("old-resolver", ID)
        .unwrap();

    assert_eq!(orchestrator.active_sessions[ID].id, worker_id);
}

#[test]
fn auto_merge_refuses_to_invent_missing_completion_proof() {
    let (repo, mut orchestrator) = worktree_stage();
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.completed_commit = None;
        Ok(())
    })
    .unwrap();
    let before = main_tip(&repo);
    assert!(!orchestrator.try_auto_merge(ID));
    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(stage.completed_commit, None);
    assert!(!stage.merged);
    assert_eq!(main_tip(&repo), before);
}

#[test]
fn resolver_exit_verifies_current_content_before_automatic_landing() {
    let (repo, mut orchestrator) = worktree_stage();
    enter_conflict(&orchestrator, &repo);
    resolve_in_worktree(&repo);
    let resolved = crate::git::get_branch_head("loom/s", repo.path()).unwrap();
    orchestrator
        .handle_merge_session_completed("session", ID)
        .unwrap();
    let stage = on_disk(&orchestrator);
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(stage.completed_commit.as_deref(), Some(resolved.as_str()));
    assert!(!worktree(&repo).exists());
}

#[test]
fn blocked_retry_without_completion_proof_never_lands() {
    let (repo, mut orchestrator) = worktree_stage();
    update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.status = StageStatus::MergeBlocked;
        stage.completed_commit = None;
        Ok(())
    })
    .unwrap();
    let before = main_tip(&repo);
    orchestrator.retry_blocked_merge(&on_disk(&orchestrator));
    let stage = on_disk(&orchestrator);
    assert_eq!(stage.completed_commit, None);
    assert!(!stage.merged);
    assert_eq!(main_tip(&repo), before);
    assert!(worktree(&repo).exists());
}

#[test]
fn recovery_never_derives_completion_proof_from_the_branch_tip() {
    use crate::orchestrator::core::recovery::Recovery;

    for merged in [false, true] {
        let (repo, mut orchestrator) = worktree_stage();
        orchestrator.graph =
            crate::plan::ExecutionGraph::build(vec![crate::plan::schema::StageDefinition {
                id: ID.into(),
                name: ID.into(),
                working_dir: ".".into(),
                ..Default::default()
            }])
            .unwrap();
        if merged {
            git_ok(repo.path(), &["merge", "--ff-only", "loom/s"]);
        }
        update_stage(ID, &orchestrator.config.work_dir, |stage| {
            stage.completed_commit = None;
            stage.merged = merged;
            Ok(())
        })
        .unwrap();
        let before = main_tip(&repo);
        orchestrator.sync_graph_with_stage_files().unwrap();
        let stage = on_disk(&orchestrator);
        assert_eq!(stage.completed_commit, None);
        // Merged, so final: recovery neither re-verifies nor reverts it.
        assert_eq!(stage.merged, merged);
        let status = if merged {
            StageStatus::Completed
        } else {
            StageStatus::NeedsHumanReview
        };
        assert_eq!(stage.status, status);
        assert_eq!(main_tip(&repo), before);
        assert!(worktree(&repo).exists());
    }
}

#[test]
fn finalization_cannot_replace_missing_or_revoked_completion_proof() {
    let (repo, mut orchestrator) = worktree_stage();
    git_ok(repo.path(), &["merge", "--ff-only", "loom/s"]);
    let mut original = on_disk(&orchestrator);
    let missing = update_stage(ID, &orchestrator.config.work_dir, |stage| {
        stage.completed_commit = None;
        Ok(())
    })
    .unwrap();
    assert!(!orchestrator.finalize_merge_resolution(&mut original, ID, "main", "resolved"));
    let mut missing = missing;
    assert!(!orchestrator.finalize_merge_resolution(&mut missing, ID, "main", "resolved"));
    let stage = on_disk(&orchestrator);
    assert_eq!(stage.completed_commit, None);
    assert!(!stage.merged);
}
