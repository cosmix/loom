use super::*;
use crate::verify::contracts::store::{FreezeRecord, FrozenContract, FREEZE_RECORD_VERSION};
use chrono::Utc;

fn entry(id: &str, file: &str, test: Option<&str>, adapter: Option<&str>) -> FrozenContract {
    FrozenContract {
        id: id.to_string(),
        adapter: adapter.map(str::to_string),
        outcome: "failed".to_string(),
        exit_code: Some(1),
        file: Some(file.to_string()),
        cwd: Some("pkg".to_string()),
        test: test.map(str::to_string),
    }
}

fn record(contracts: Vec<FrozenContract>) -> FreezeRecord {
    FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "s".to_string(),
        session_id: "session-1".to_string(),
        frozen_at: Utc::now(),
        base: "a".repeat(40),
        files: Vec::new(),
        contracts,
    }
}

fn spec(test: Option<&str>, runner: Option<&str>) -> ContractSpec {
    ContractSpec {
        id: "c".to_string(),
        file: "./tests/c.rs".to_string(),
        test: test.map(str::to_string),
        runner: runner.map(str::to_string),
        cwd: None,
        scenario: "s".to_string(),
        rejects: "r".to_string(),
    }
}

#[test]
fn planner_runner_is_kept_over_the_recorded_adapter() {
    let record = record(vec![entry("c", "tests/c.rs", Some("t"), Some("vitest"))]);
    let kept = effective_contract(&record, &spec(None, Some("cargo")));
    assert_eq!(kept.runner.as_deref(), Some("cargo"));
    let filled = effective_contract(&record, &spec(None, None));
    assert_eq!(filled.runner.as_deref(), Some("vitest"));
}

#[test]
fn recorded_test_beats_a_hint_and_fills_a_missing_test() {
    let record = record(vec![entry("c", "tests/c.rs", Some("recorded"), None)]);
    let beaten = effective_contract(&record, &spec(Some("hint"), None));
    assert_eq!(beaten.test.as_deref(), Some("recorded"));
    let filled = effective_contract(&record, &spec(None, None));
    assert_eq!(filled.test.as_deref(), Some("recorded"));
    let unrecorded = FreezeRecord {
        contracts: vec![entry("c", "tests/c.rs", None, None)],
        ..record
    };
    let hinted = effective_contract(&unrecorded, &spec(Some("hint"), None));
    assert_eq!(hinted.test.as_deref(), Some("hint"));
}

#[test]
fn an_entry_for_another_file_is_not_used() {
    let record = record(vec![entry(
        "c",
        "tests/other.rs",
        Some("recorded"),
        Some("vitest"),
    )]);
    let given = spec(Some("hint"), None);
    assert_eq!(effective_contract(&record, &given), given);
}

#[test]
fn no_entry_returns_the_spec_unchanged() {
    let given = spec(None, None);
    assert_eq!(effective_contract(&record(Vec::new()), &given), given);
}

#[test]
fn recorded_cwd_follows_the_same_entry() {
    let record = record(vec![entry("c", "tests/c.rs", None, None)]);
    assert_eq!(recorded_cwd(&record, &spec(None, None)), Some("pkg"));
}
