//! The freeze flags: how they parse, how they travel, and what the writer's
//! own CLI refuses or keeps before anything is relayed.

use std::collections::BTreeMap;
use std::time::Duration;

use tempfile::TempDir;

use super::super::freeze_payload;
use super::super::freeze_pending::{record_sent, refuse_other_flags};
use super::*;
use crate::fs::stage_request::StageRequest;
use crate::host_verify::wait::{wait_or_request, WaitSite};
use crate::models::stage::Stage;
use crate::plan::schema::ContractChoice;
use crate::relay::{decode_payload, HostJobRequest, RequestPayload};
use crate::verify::contracts::test_support::{
    contract, contract_worktree, write_files, CONTRACT_FILE,
};

fn choice(test: &str) -> ContractChoice {
    ContractChoice {
        test: Some(test.to_string()),
        ..ContractChoice::default()
    }
}

fn choices_of(entries: &[(&str, &str)]) -> ContractChoices {
    entries
        .iter()
        .map(|(id, test)| (id.to_string(), choice(test)))
        .collect()
}

#[test]
fn the_relay_payload_decodes_back_to_the_freeze_request() {
    let request = HostJobRequest {
        head: "a".repeat(40),
        tree_digest: "b".repeat(64),
        no_cache: false,
        criteria: None,
    };
    let choices = choices_of(&[("a", "x::y")]);
    let payload = freeze_payload(&request, &choices).unwrap();
    let RequestPayload::FreezeContracts(decoded) =
        decode_payload(RequestKind::FreezeContracts, &payload).unwrap()
    else {
        panic!("decoded as another request");
    };
    assert_eq!(
        decoded,
        StageRequest::FreezeContracts {
            head: request.head,
            tree_digest: request.tree_digest,
            choices,
        }
    );
}

#[test]
fn an_empty_choices_map_is_not_serialised_and_entries_round_trip() {
    let bare = StageRequest::FreezeContracts {
        head: "a".repeat(40),
        tree_digest: "b".repeat(64),
        choices: BTreeMap::new(),
    };
    let value = serde_json::to_value(&bare).unwrap();
    assert!(value.get("choices").is_none(), "{value}");

    let named = StageRequest::FreezeContracts {
        head: "a".repeat(40),
        tree_digest: "b".repeat(64),
        choices: choices_of(&[("a", "x::y")]),
    };
    let value = serde_json::to_value(&named).unwrap();
    assert_eq!(
        serde_json::from_value::<StageRequest>(value).unwrap(),
        named
    );
}

#[test]
fn the_relay_decode_bounds_the_choices_keys() {
    let freeze = |choices: serde_json::Value| {
        serde_json::json!({
            "request": "freeze_contracts",
            "head": "a".repeat(40),
            "tree_digest": "b".repeat(64),
            "choices": choices,
        })
    };
    let many: serde_json::Map<String, serde_json::Value> = (0..65)
        .map(|n| (format!("c{n}"), serde_json::json!({"test": "t"})))
        .collect();
    let exactly: serde_json::Map<String, serde_json::Value> = many
        .iter()
        .take(64)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert!(decode_payload(RequestKind::FreezeContracts, &freeze(many.into())).is_err());
    assert!(decode_payload(RequestKind::FreezeContracts, &freeze(exactly.into())).is_ok());
    let escaping = freeze(serde_json::json!({"../x": {"test": "t"}}));
    let error = decode_payload(RequestKind::FreezeContracts, &escaping).unwrap_err();
    assert!(format!("{error:#}").contains("contract ids"), "{error:#}");
}

/// A stage with two contracts and no plan hint, over a real worktree.
struct Fx {
    _temp: TempDir,
    site: ContractSite,
}

fn fixture() -> Fx {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path().join("repo");
    let worktree = contract_worktree(&repo_root, "s1");
    write_files(
        &worktree,
        &[("tests/other_contract.rs", "#[test]\nfn other() {}\n")],
    );
    let workspace = crate::fs::work_dir::WorkDir::new(&repo_root).unwrap();
    workspace.adopt_existing().unwrap();
    let root = worktree.canonicalize().unwrap();
    let hintless = |id: &str, file: &str| ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        test: None,
        ..contract()
    };
    let stage = Stage {
        id: "s1".to_string(),
        contracts: vec![
            hintless("first", CONTRACT_FILE),
            hintless("second", "tests/other_contract.rs"),
        ],
        ..Stage::default()
    };
    let site = ContractSite::resolved(workspace.root().to_path_buf(), stage, root.clone(), root);
    Fx { _temp: temp, site }
}

fn tests_of(specs: &[ContractSpec]) -> Vec<Option<&str>> {
    specs.iter().map(|spec| spec.test.as_deref()).collect()
}

#[test]
fn a_refreeze_keeps_the_tests_an_earlier_freeze_named() {
    let fx = fixture();
    let saved = choices_of(&[("first", "x_contract::rejects_first")]);
    choices::save_choices(&fx.site.work_dir, "s1", &saved).unwrap();

    let specs = checked_overlay(
        &fx.site,
        &choices_of(&[("second", "other::rejects_second")]),
    )
    .expect("the saved test and the new flag cover both contracts");
    assert_eq!(
        tests_of(&specs),
        [
            Some("x_contract::rejects_first"),
            Some("other::rejects_second")
        ]
    );
}

#[test]
fn a_contract_left_without_a_test_or_given_a_flag_value_is_refused_locally() {
    let fx = fixture();
    let only_first = choices_of(&[("first", "x_contract::rejects_first")]);
    let error = checked_overlay(&fx.site, &only_first).unwrap_err();
    assert!(format!("{error:#}").contains("second"), "{error:#}");

    let both = choices_of(&[("first", "--help"), ("second", "other::rejects_second")]);
    assert!(checked_overlay(&fx.site, &both).is_err());
}

#[test]
fn a_saved_choice_for_a_contract_the_plan_no_longer_has_is_pruned_not_refused() {
    let fx = fixture();
    let saved = choices_of(&[
        ("gone", "old::test"),
        ("first", "x_contract::rejects_first"),
    ]);
    choices::save_choices(&fx.site.work_dir, "s1", &saved).unwrap();

    let specs = checked_overlay(
        &fx.site,
        &choices_of(&[("second", "other::rejects_second")]),
    )
    .expect("a stale id never refuses a freeze");
    assert_eq!(specs.len(), 2);
}

fn wait_site<'a>(fx: &'a Fx, scratch: &'a std::path::Path) -> WaitSite<'a> {
    WaitSite {
        work_dir: &fx.site.work_dir,
        stage_id: "s1",
        session_id: "session-1",
        checkout: &fx.site.worktree_root,
        knowledge: false,
        kind: JobKind::Freeze,
        no_cache: false,
        criteria: None,
        scratch_dir: scratch,
        budget: Duration::ZERO,
    }
}

/// A pending freeze request "r1" sent with `sent`, as the relay route leaves it.
fn pending(fx: &Fx, scratch: &std::path::Path, sent: &ContractChoices) {
    let site = wait_site(fx, scratch);
    let mut relay = |_: &HostJobRequest| Ok("r1".to_string());
    wait_or_request(&site, &mut relay, &mut Vec::new()).unwrap();
    record_sent(&site, "r1", sent);
}

#[test]
fn a_rerun_under_other_flags_than_a_pending_freeze_is_refused() {
    let fx = fixture();
    let scratch = TempDir::new().unwrap();
    pending(&fx, scratch.path(), &choices_of(&[("first", "x::one")]));
    let site = wait_site(&fx, scratch.path());

    let error = refuse_other_flags(&site, &choices_of(&[("first", "x::two")])).unwrap_err();
    assert!(
        format!("{error:#}").contains("a freeze under other flags is pending"),
        "{error:#}"
    );
    let added = choices_of(&[("second", "y::one")]);
    assert!(refuse_other_flags(&site, &added).is_err());
}

#[test]
fn a_rerun_repeating_the_same_flags_or_none_follows_the_pending_freeze() {
    let fx = fixture();
    let scratch = TempDir::new().unwrap();
    pending(&fx, scratch.path(), &choices_of(&[("first", "x::one")]));
    let site = wait_site(&fx, scratch.path());

    refuse_other_flags(&site, &choices_of(&[("first", "x::one")])).expect("same flags");
    refuse_other_flags(&site, &BTreeMap::new()).expect("no flags");
}

#[test]
fn a_missing_companion_file_or_no_pending_request_refuses_nothing() {
    let fx = fixture();
    let scratch = TempDir::new().unwrap();
    let site = wait_site(&fx, scratch.path());
    refuse_other_flags(&site, &choices_of(&[("first", "x::one")])).expect("no request yet");

    let mut relay = |_: &HostJobRequest| Ok("r1".to_string());
    wait_or_request(&site, &mut relay, &mut Vec::new()).unwrap();
    refuse_other_flags(&site, &choices_of(&[("first", "x::one")])).expect("no companion file");

    record_sent(
        &site,
        "another-request",
        &choices_of(&[("first", "x::old")]),
    );
    refuse_other_flags(&site, &choices_of(&[("first", "x::one")])).expect("names another request");
}
