use super::*;

#[test]
fn the_resolver_exit_routes_failed_verification_to_review() {
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
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit.as_deref(), Some(original.as_str()));
    assert_eq!(main_tip(&repo), before);
    assert!(worktree(&repo).exists());
    let reason = stage.review_reason.unwrap();
    assert!(reason.contains("Acceptance criteria failed"), "{reason}");
    assert!(reason.contains("loom stage complete s"));
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
        assert!(!stage.merged);
        assert_eq!(stage.status, StageStatus::NeedsHumanReview);
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
