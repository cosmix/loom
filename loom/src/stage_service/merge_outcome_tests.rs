use super::*;
use crate::orchestrator::auto_merge::AutoMergeResult;
use crate::orchestrator::progressive_merge::ProgressiveMergeResult;

fn kind(outcome: MergeDisposition) -> &'static str {
    match outcome {
        MergeDisposition::Landed { .. } => "landed",
        MergeDisposition::AlreadyLanded => "already",
        MergeDisposition::Conflict(_) => "conflict",
        MergeDisposition::Blocked(_) => "blocked",
        MergeDisposition::Held(_) => "held",
        MergeDisposition::MissingBranch => "missing-branch",
        MergeDisposition::MissingWorktree => "missing-worktree",
    }
}

fn git_results() -> Vec<MergeResult> {
    vec![
        MergeResult::Success {
            files_changed: 1,
            insertions: 2,
            deletions: 3,
            stash: None,
        },
        MergeResult::AlreadyUpToDate,
        MergeResult::Conflict {
            conflicting_files: vec!["a".into()],
        },
        MergeResult::Blocked(MergeBlock::TargetMoved),
        MergeResult::Held {
            reason: "hold".into(),
        },
    ]
}

fn daemon_results() -> Vec<AutoMergeResult> {
    vec![
        AutoMergeResult::Success {
            files_changed: 1,
            insertions: 2,
            deletions: 3,
            stash: None,
        },
        AutoMergeResult::AlreadyUpToDate,
        AutoMergeResult::Conflict {
            conflicting_files: vec!["a".into()],
        },
        AutoMergeResult::Blocked(MergeBlock::TargetMoved),
        AutoMergeResult::Held {
            reason: "hold".into(),
        },
    ]
}

fn progressive_results() -> Vec<ProgressiveMergeResult> {
    vec![
        ProgressiveMergeResult::Success {
            files_changed: 1,
            stash: None,
        },
        ProgressiveMergeResult::AlreadyMerged,
        ProgressiveMergeResult::Conflict {
            conflicting_files: vec!["a".into()],
        },
        ProgressiveMergeResult::Blocked(MergeBlock::TargetMoved),
        ProgressiveMergeResult::Held {
            reason: "hold".into(),
        },
    ]
}

#[test]
fn adapters_classify_equivalent_results_identically() {
    let expected = vec!["landed", "already", "conflict", "blocked", "held"];
    assert_eq!(
        git_results()
            .into_iter()
            .map(|v| kind(v.into()))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        daemon_results()
            .into_iter()
            .map(|v| kind(v.into()))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        progressive_results()
            .into_iter()
            .map(|v| kind(v.into()))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(kind(AutoMergeResult::NoWorktree.into()), "missing-worktree");
    assert_eq!(
        kind(ProgressiveMergeResult::NoBranch.into()),
        "missing-branch"
    );
}

#[test]
fn failures_distinguish_signing_from_other_errors() {
    let signing = anyhow::Error::new(CommitTreeError {
        signing: true,
        detail: "key unavailable".into(),
    })
    .context("merge failed");
    assert!(
        matches!(classify_failure("s", &signing), MergeFailure::Signing(reason) if reason.contains("key unavailable"))
    );
    let other = anyhow::Error::new(CommitTreeError {
        signing: false,
        detail: "bad tree".into(),
    });
    assert!(
        matches!(classify_failure("s", &other), MergeFailure::Infrastructure(reason) if reason.contains("bad tree"))
    );
}

#[test]
fn stale_completion_proof_cannot_apply_a_merge_failure() {
    let mut stage = Stage {
        id: "s".into(),
        completed_commit: Some("new".into()),
        ..Stage::default()
    };
    assert!(check_claim(&stage, Some("old")).is_err());
    assert!(!stage.merged);
    stage.merged = true;
    assert!(check_claim(&stage, Some("new")).is_err());
}

#[test]
fn a_replacement_owner_cannot_be_overwritten() {
    let expected = Stage {
        id: "s".into(),
        session: Some("old".into()),
        ..Stage::default()
    };
    let mut current = expected.clone();
    current.session = Some("new".into());
    assert!(check_stage(&current, &expected).is_err());
    current.session = expected.session.clone();
    current.attempt_started_at = Some(chrono::Utc::now());
    assert!(check_stage(&current, &expected).is_err());
    current.attempt_started_at = expected.attempt_started_at;
    current.fix_attempts += 1;
    assert!(check_stage(&current, &expected).is_err());
}

#[test]
fn equivalent_authorities_apply_the_same_stage_changes() {
    use crate::models::stage::StageStatus;
    for authority in [
        ConflictAuthority::Manual,
        ConflictAuthority::Session,
        ConflictAuthority::Daemon,
    ] {
        let mut stage = Stage {
            id: "s".into(),
            status: StageStatus::Executing,
            ..Stage::default()
        };
        apply_conflict(&mut stage, authority).unwrap();
        assert_eq!(stage.status, StageStatus::MergeConflict);
        assert!(stage.merge_conflict);
    }
    let mut blocked = Stage {
        id: "s".into(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    apply_block(&mut blocked, MergeBlock::TargetMoved);
    assert_eq!(blocked.status, StageStatus::MergeBlocked);
    assert_eq!(blocked.merge.block, Some(MergeBlock::TargetMoved));
    let mut held = Stage {
        id: "s".into(),
        status: StageStatus::Executing,
        ..Stage::default()
    };
    apply_review(&mut held, "hold");
    assert_eq!(held.status, StageStatus::NeedsHumanReview);
    assert_eq!(held.review_reason.as_deref(), Some("hold"));
}

/// `loom stage merge` retries a blocked merge too: a failed attempt keeps the
/// block, and a stage with none takes the ordinary failure.
#[test]
fn a_failed_retry_keeps_a_typed_block() {
    let mut blocked = Stage {
        status: crate::models::stage::StageStatus::MergeBlocked,
        ..Stage::default()
    };
    blocked.merge.block = Some(MergeBlock::TargetMoved);
    apply_retry_failure(&mut blocked, "git failed").unwrap();
    assert_eq!(blocked.merge.block, Some(MergeBlock::TargetMoved));
    assert_eq!(
        blocked.failure_info.map(|info| info.evidence),
        Some(vec!["git failed".to_string()])
    );

    let mut completed = Stage {
        status: crate::models::stage::StageStatus::Completed,
        ..Stage::default()
    };
    apply_retry_failure(&mut completed, "git failed").unwrap();
    assert_eq!(
        completed.status,
        crate::models::stage::StageStatus::MergeBlocked
    );
    assert_eq!(completed.merge.block, None);
    assert!(completed.failure_info.is_some());
}
