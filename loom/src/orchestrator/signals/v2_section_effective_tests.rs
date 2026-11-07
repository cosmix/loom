//! The implementer's frozen-contracts table shows the test the freeze ran,
//! read from the freeze record over the plan's hint.

use super::*;
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::store::{
    write_freeze, FreezeRecord, FrozenContract, FREEZE_RECORD_VERSION,
};
use crate::verify::contracts::test_support::contract;

fn stage_with(spec: ContractSpec) -> Stage {
    Stage {
        id: "s1".to_string(),
        plan_version: 2,
        stage_type: StageType::Standard,
        contracts: vec![spec],
        ..Stage::default()
    }
}

/// Freeze stage `s1` with one entry for the test contract, which ran `test`.
fn freeze_with_test(work_dir: &Path, test: Option<&str>) {
    let spec = contract();
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "session-1".to_string(),
        frozen_at: chrono::Utc::now(),
        base: "main".to_string(),
        files: Vec::new(),
        contracts: vec![FrozenContract {
            id: spec.id,
            adapter: Some("cargo-test".to_string()),
            outcome: "failed".to_string(),
            exit_code: Some(101),
            file: Some(spec.file),
            cwd: Some(".".to_string()),
            test: test.map(str::to_string),
        }],
    };
    write_freeze(work_dir, &record, &[]).unwrap();
}

fn render(stage: &Stage, work_dir: &Path) -> String {
    let mut content = String::new();
    append_v2_section(&mut content, stage, work_dir);
    content
}

#[test]
fn the_table_shows_the_recorded_test_over_the_plan_hint() {
    let temp = tempfile::tempdir().unwrap();
    freeze_with_test(temp.path(), Some("writer::rejects_x"));

    let content = render(&stage_with(contract()), temp.path());

    assert!(
        content.contains("| `rejects-x` | `tests/x_contract.rs` | `writer::rejects_x` |"),
        "{content}"
    );
    assert!(!content.contains("`tests::rejects_x`"), "{content}");
}

#[test]
fn the_table_shows_the_recorded_test_where_the_plan_had_no_hint() {
    let temp = tempfile::tempdir().unwrap();
    freeze_with_test(temp.path(), Some("writer::rejects_x"));
    let spec = ContractSpec {
        test: None,
        ..contract()
    };

    let content = render(&stage_with(spec), temp.path());

    assert!(
        content.contains("| `rejects-x` | `tests/x_contract.rs` | `writer::rejects_x` |"),
        "{content}"
    );
}

#[test]
fn a_record_with_no_test_leaves_the_plan_hint() {
    let temp = tempfile::tempdir().unwrap();
    freeze_with_test(temp.path(), None);

    let content = render(&stage_with(contract()), temp.path());

    assert!(
        content.contains("| `rejects-x` | `tests/x_contract.rs` | `tests::rejects_x` |"),
        "{content}"
    );
}
