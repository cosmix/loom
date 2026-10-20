use super::*;
use crate::commands::stage::progressive_complete::Landing;
use crate::models::failure::FailureType;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage};

fn executing_stage() -> Stage {
    Stage {
        id: "s".to_string(),
        status: StageStatus::Executing,
        ..Stage::default()
    }
}

#[test]
fn an_untyped_merge_failure_drops_a_stale_typed_block() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut stage = executing_stage();
    stage.merge.block = Some(MergeBlock::TargetMoved);
    save_stage(&stage, dir.path()).unwrap();

    mark_blocked(
        &mut stage,
        dir.path(),
        &Claim::new(&executing_stage(), Landing::Session),
        "boom",
    )
    .unwrap();

    let saved = load_stage("s", dir.path()).unwrap();
    assert_eq!(saved.status, StageStatus::MergeBlocked);
    assert_eq!(saved.merge.block, None);
    assert_eq!(stage.merge.block, None);
}

#[test]
fn an_untyped_merge_failure_records_its_reason() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut stage = executing_stage();
    save_stage(&stage, dir.path()).unwrap();

    mark_blocked(
        &mut stage,
        dir.path(),
        &Claim::new(&executing_stage(), Landing::Session),
        "first line\nsecond line",
    )
    .unwrap();

    let saved = load_stage("s", dir.path()).unwrap();
    let info = saved.failure_info.expect("the reason is recorded");
    assert_eq!(info.failure_type, FailureType::InfrastructureError);
    assert_eq!(info.evidence, vec!["first line", "second line"]);
    assert!(stage.failure_info.is_some());
}
