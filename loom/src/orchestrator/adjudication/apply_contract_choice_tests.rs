//! The test a contract re-freeze records (`apply_contract.rs`): the record's
//! for a contract the verdict did not touch, the amended spec's for one it
//! replaced or inserted with a test, the record's again when the amendment
//! names none.

use chrono::Utc;
use std::collections::BTreeSet;

use super::*;
use crate::verify::contracts::store::{FrozenContract, FREEZE_RECORD_VERSION};

fn spec(id: &str, test: Option<&str>) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: format!("tests/{id}_contract.rs"),
        test: test.map(str::to_string),
        runner: None,
        cwd: None,
        scenario: "x arrives".to_string(),
        rejects: "accepting x".to_string(),
    }
}

/// A record whose entries ran the test the writer chose for each id.
fn record(entries: &[(&str, &str)]) -> FreezeRecord {
    FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: Vec::new(),
        contracts: entries
            .iter()
            .map(|(id, test)| FrozenContract {
                id: id.to_string(),
                adapter: Some("cargo-test".to_string()),
                outcome: "failed".to_string(),
                exit_code: Some(101),
                file: Some(format!("tests/{id}_contract.rs")),
                cwd: Some(".".to_string()),
                test: Some(test.to_string()),
            })
            .collect(),
    }
}

fn ids(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn an_untouched_contract_keeps_the_record_test_over_the_plan_hint() {
    let record = record(&[("a", "writer::a")]);

    let refrozen = refrozen_spec(&record, &spec("a", Some("hint::a")), &ids(&["b"]));

    assert_eq!(refrozen.test.as_deref(), Some("writer::a"));
    assert_eq!(refrozen.runner.as_deref(), Some("cargo-test"));
}

#[test]
fn a_touched_contract_takes_its_amended_test() {
    let record = record(&[("a", "writer::a")]);

    let refrozen = refrozen_spec(&record, &spec("a", Some("amended::a")), &ids(&["a"]));

    assert_eq!(refrozen.test.as_deref(), Some("amended::a"));
}

#[test]
fn a_touched_contract_without_a_test_keeps_the_record_test() {
    let record = record(&[("a", "writer::a")]);

    let refrozen = refrozen_spec(&record, &spec("a", None), &ids(&["a"]));

    assert_eq!(refrozen.test.as_deref(), Some("writer::a"));
}

#[test]
fn an_inserted_contract_the_record_lacks_records_its_amended_test() {
    let record = record(&[("a", "writer::a")]);

    let refrozen = refrozen_spec(&record, &spec("z", Some("amended::z")), &ids(&["z"]));

    assert_eq!(refrozen.test.as_deref(), Some("amended::z"));
}

#[test]
fn a_patch_names_the_contract_it_replaces_or_inserts_and_a_delete_none() {
    let value = "id: a\nfile: tests/a_contract.rs\ntest: t::a\nscenario: s\nrejects: r\n";
    let replace = AmendmentPatch::Replace {
        index: 0,
        value: value.to_string(),
    };
    let insert = AmendmentPatch::Insert {
        index: 1,
        value: value.to_string(),
    };

    assert_eq!(amended_ids(&replace), ids(&["a"]));
    assert_eq!(amended_ids(&insert), ids(&["a"]));
    assert!(amended_ids(&AmendmentPatch::Delete { index: 0 }).is_empty());
}
