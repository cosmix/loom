use super::*;

fn stage_in(status: StageStatus) -> Stage {
    Stage {
        id: "s".to_string(),
        status,
        ..Stage::default()
    }
}

#[test]
fn block_merge_records_the_reason_and_status() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::TargetMoved);
    assert_eq!(stage.status, StageStatus::MergeBlocked);
    assert_eq!(stage.merge.block, Some(MergeBlock::TargetMoved));
    let info = stage.failure_info.expect("failure_info shows the sentence");
    assert_eq!(info.evidence, vec![MergeBlock::TargetMoved.to_string()]);
}

#[test]
fn record_merge_stash_keeps_the_outcome() {
    let mut stage = stage_in(StageStatus::Completed);
    let stash = StashReapply {
        backup_ref: "refs/loom/autostash/s-1".to_string(),
        restored: true,
    };
    stage.record_merge_stash(stash.clone());
    assert_eq!(stage.merge.stash, Some(stash));
}

#[test]
fn block_merge_stash_not_restored_records_the_unrestored_stash() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::StashNotRestored {
        backup_ref: "refs/loom/autostash/s-1".to_string(),
    });
    assert_eq!(
        stage.merge.stash,
        Some(StashReapply {
            backup_ref: "refs/loom/autostash/s-1".to_string(),
            restored: false,
        })
    );
    stage.block_merge(MergeBlock::TargetMoved);
    assert!(stage.merge.stash.is_some(), "a later block keeps the note");
}

fn stash(backup_ref: &str, restored: bool) -> StashReapply {
    StashReapply {
        backup_ref: backup_ref.to_string(),
        restored,
    }
}

#[test]
fn a_restored_stash_note_keeps_an_unrestored_one() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.record_merge_stash(stash("ref1", false));
    stage.record_merge_stash(stash("ref2", true));
    assert_eq!(stage.merge.stash, Some(stash("ref1", false)));
}

#[test]
fn an_unrestored_stash_note_replaces_any_note() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.record_merge_stash(stash("ref1", false));
    stage.record_merge_stash(stash("ref2", false));
    assert_eq!(stage.merge.stash, Some(stash("ref2", false)));
    stage.record_merge_stash(stash("ref3", true));
    stage.merge.stash = Some(stash("ref4", true));
    stage.record_merge_stash(stash("ref5", true));
    assert_eq!(stage.merge.stash, Some(stash("ref5", true)));
}

#[test]
fn every_unrestored_backup_ref_is_kept_once() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.record_merge_stash(stash("ref1", false));
    stage.record_merge_stash(stash("ref2", false));
    stage.record_merge_stash(stash("ref2", false));
    stage.record_merge_stash(stash("ref3", true));
    assert_eq!(stage.merge.unrestored, vec!["ref1", "ref2"]);
    assert_eq!(stage.merge.stash, Some(stash("ref2", false)));
}

#[test]
fn leaving_merge_blocked_by_transition_clears_the_block_and_keeps_the_stash() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.record_merge_stash(stash("ref1", false));
    stage.block_merge(MergeBlock::TargetMoved);
    assert_eq!(stage.merge.block, Some(MergeBlock::TargetMoved));
    stage.try_transition(StageStatus::Queued).unwrap();
    assert_eq!(stage.merge.block, None);
    assert!(stage.failure_info.is_none());
    assert_eq!(stage.merge.stash, Some(stash("ref1", false)));
}

#[test]
fn a_forced_status_change_clears_the_block() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::TargetMoved);
    stage.force_status_with_reason(StageStatus::NeedsHumanReview, "test");
    assert_eq!(stage.merge.block, None);
    assert!(stage.failure_info.is_none());
}

#[test]
fn block_merge_from_a_status_without_the_edge_forces_it() {
    let mut stage = stage_in(StageStatus::Queued);
    stage.block_merge(MergeBlock::TargetMoved);
    assert_eq!(stage.status, StageStatus::MergeBlocked);
}

#[test]
fn enter_merge_conflict_clears_the_block() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::TargetMoved);
    stage.enter_merge_conflict();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert!(stage.merge_conflict);
    assert_eq!(stage.merge.block, None);
}

#[test]
fn clear_merge_block_clears_the_failure_info_the_block_wrote() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::TargetMoved);
    stage.clear_merge_block();
    assert_eq!(stage.merge.block, None);
    assert!(stage.failure_info.is_none());
}

#[test]
fn clear_merge_block_keeps_an_unrelated_failure_info() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.failure_info = Some(FailureInfo {
        failure_type: FailureType::InfrastructureError,
        detected_at: Utc::now(),
        evidence: vec!["disk full".to_string()],
    });
    stage.clear_merge_block();
    assert_eq!(stage.failure_info.expect("kept").evidence, ["disk full"]);
}

#[test]
fn a_missing_completed_commit_is_recorded_and_an_existing_one_kept() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.record_completed_commit_if_missing(Some("aaa"));
    assert_eq!(stage.completed_commit.as_deref(), Some("aaa"));
    stage.record_completed_commit_if_missing(Some("bbb"));
    stage.record_completed_commit_if_missing(None);
    assert_eq!(stage.completed_commit.as_deref(), Some("aaa"));
}

#[test]
fn zero_commit_reason_names_the_branch_target_and_requeue_command() {
    let reason = Stage::zero_commit_reason("s1", "main");
    assert!(reason.contains("loom/s1"), "{reason}");
    assert!(reason.contains("zero commits beyond main"), "{reason}");
    assert!(
        reason.contains("loom stage human-review s1 --approve"),
        "{reason}"
    );
}

#[test]
fn route_to_review_records_the_reason_and_clears_the_block() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::TargetMoved);
    stage.route_to_review("touches .claude/");
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(stage.review_reason.as_deref(), Some("touches .claude/"));
    assert_eq!(stage.merge.block, None);
}

#[test]
fn merge_block_round_trips_and_is_omitted_when_absent() {
    let mut stage = stage_in(StageStatus::Completed);
    let plain = crate::yaml::to_string(&stage).unwrap();
    assert!(!plain.contains("merge_block"), "{plain}");
    let back: Stage = crate::yaml::from_str(&plain).unwrap();
    assert_eq!(back.merge.block, None);

    stage.block_merge(MergeBlock::UncommittedOverlap {
        paths: vec!["a.txt".to_string()],
    });
    let text = crate::yaml::to_string(&stage).unwrap();
    let back: Stage = crate::yaml::from_str(&text).unwrap();
    assert_eq!(back.merge.block, stage.merge.block);
}

#[test]
fn merge_record_keeps_top_level_keys_and_round_trips() {
    let mut stage = stage_in(StageStatus::Completed);
    let plain = crate::yaml::to_string(&stage).unwrap();
    assert!(!plain.contains("merge_block:"), "{plain}");
    assert!(!plain.contains("merge_stash:"), "{plain}");
    assert!(!plain.contains("merge_unrestored_stashes:"), "{plain}");

    stage.merge = MergeRecord {
        block: Some(MergeBlock::TargetMoved),
        stash: Some(StashReapply {
            backup_ref: "refs/loom/autostash/x".to_string(),
            restored: false,
        }),
        unrestored: vec!["refs/loom/autostash/x".to_string()],
    };
    let text = crate::yaml::to_string(&stage).unwrap();
    assert!(
        text.lines().any(|l| l.starts_with("merge_block:")),
        "{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("merge_stash:")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("merge_unrestored_stashes:")),
        "{text}"
    );
    let back: Stage = crate::yaml::from_str(&text).unwrap();
    assert_eq!(back.merge, stage.merge);
}
