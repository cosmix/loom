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
fn leaving_merge_blocked_by_transition_clears_the_block() {
    let mut stage = stage_in(StageStatus::Completed);
    stage.block_merge(MergeBlock::TargetMoved);
    assert_eq!(stage.merge.block, Some(MergeBlock::TargetMoved));
    stage.try_transition(StageStatus::Queued).unwrap();
    assert_eq!(stage.merge.block, None);
    assert!(stage.failure_info.is_none());
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

    stage.merge = MergeRecord {
        block: Some(MergeBlock::TargetMoved),
        conflict_since: Some(chrono::Utc::now()),
        ..MergeRecord::default()
    };
    let text = crate::yaml::to_string(&stage).unwrap();
    assert!(
        text.lines().any(|l| l.starts_with("merge_block:")),
        "{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("merge_conflict_since:")),
        "{text}"
    );
    let back: Stage = crate::yaml::from_str(&text).unwrap();
    assert_eq!(back.merge, stage.merge);
}

#[test]
fn only_an_incomplete_restore_whose_quarantine_exists_holds_displaced_files() {
    let dir = tempfile::tempdir().unwrap();
    let held = |quarantine: std::path::PathBuf| MergeBlock::CheckoutRestoreIncomplete {
        quarantine,
        failures: vec![],
        detail: None,
    };
    assert!(held(dir.path().to_path_buf()).holds_displaced_files());
    assert!(!held(dir.path().join("gone")).holds_displaced_files());
    for other in [
        MergeBlock::TargetMoved,
        MergeBlock::UncommittedOverlap {
            paths: vec![dir.path().display().to_string()],
        },
        MergeBlock::FastForwardRefused {
            detail: dir.path().display().to_string(),
        },
    ] {
        assert!(!other.holds_displaced_files(), "{other:?}");
    }
}

#[test]
fn only_a_held_quarantine_tells_the_operator_to_recover_the_files() {
    let dir = tempfile::tempdir().unwrap();
    let held = |quarantine: std::path::PathBuf| MergeBlock::CheckoutRestoreIncomplete {
        quarantine,
        failures: vec![],
        detail: None,
    };
    let note = held(dir.path().to_path_buf()).retry_note();
    assert!(note.contains("No merge of this stage runs"), "{note}");
    assert!(!note.contains("every tick"), "{note}");
    let held_with_remedy = MergeBlock::CheckoutRestoreIncomplete {
        quarantine: dir.path().to_path_buf(),
        failures: vec![],
        detail: Some("delete it".to_string()),
    };
    let note = held_with_remedy.retry_note();
    assert!(note.contains("No merge of this stage runs"), "{note}");
    assert!(!note.contains("delete"), "{note}");
    for daemon_retried in [held(dir.path().join("gone")), MergeBlock::TargetMoved] {
        assert_eq!(
            daemon_retried.retry_note(),
            "Loom retries the merge every tick"
        );
    }
}
