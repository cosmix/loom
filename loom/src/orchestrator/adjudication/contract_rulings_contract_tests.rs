//! Contracts for per-contract rulings on a contract dispute:
//! `an-accept-that-changes-nothing-is-refused`,
//! `a-patch-outside-the-accepted-contracts-is-refused` and
//! `a-contract-file-is-written-only-when-all-its-contracts-are-accepted`.
use crate::models::dispute::{Citation, ContractRuling, ContractRulingKind};
use crate::orchestrator::adjudication::apply_contract_rulings::files_to_write;
use crate::orchestrator::adjudication::verdict_contract::validate_contract_rulings;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const ALPHA: &str = "alpha-contract";
const BETA: &str = "beta-contract";
const GAMMA: &str = "gamma-contract";

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn set(values: &[&str]) -> BTreeSet<String> {
    ids(values).into_iter().collect()
}

fn index(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
    entries
        .iter()
        .map(|(id, position)| ((*id).to_string(), *position))
        .collect()
}

fn ruling_json(contract: &str, ruling: &str) -> Value {
    json!({
        "contract": contract,
        "ruling": ruling,
        "reasoning": format!("the spec for {contract} supports {ruling}"),
        "citations": [{"file": "loom/src/lib.rs", "excerpt": "pub mod verify;", "claim": "cited"}],
    })
}

fn verdict(rulings: Vec<Value>, patches: Option<Value>) -> Value {
    let mut value = json!({"verdict": "contract-rulings", "rulings": rulings});
    if let Some(patches) = patches {
        value["plan_patch"] = json!({"field": "contracts", "patches": patches});
    }
    value
}

fn error_text(result: Result<Vec<ContractRuling>, String>, case: &str) -> String {
    match result {
        Ok(_) => panic!("{case}: the verdict must be refused"),
        Err(text) => text,
    }
}

#[test]
fn an_accept_that_changes_nothing_is_refused() {
    let disputed = ids(&[ALPHA, BETA]);
    let contract_index = index(&[(ALPHA, 0), (BETA, 1)]);
    let json = verdict(
        vec![ruling_json(ALPHA, "accept"), ruling_json(BETA, "accept")],
        None,
    );

    let both_replaced =
        validate_contract_rulings(&json, &disputed, &set(&[ALPHA, BETA]), &contract_index);
    match both_replaced {
        Ok(rulings) => assert_eq!(rulings.len(), 2, "one ruling per disputed contract"),
        Err(text) => panic!("accepts that each replace a file are valid: {text}"),
    }

    let result = validate_contract_rulings(&json, &disputed, &set(&[ALPHA]), &contract_index);
    let text = error_text(result, "an accept with no replacement and no patch");
    assert!(text.contains("changes nothing"), "refusal text: {text}");
    assert!(text.contains(BETA), "refusal names the contract: {text}");
}

#[test]
fn a_patch_outside_the_accepted_contracts_is_refused() {
    let disputed = ids(&[ALPHA]);
    let replaced = BTreeSet::new();
    let contract_index = index(&[(ALPHA, 0), (GAMMA, 1)]);
    let delete = |at: usize| Some(json!([{"op": "delete", "index": at}]));

    let accepted = verdict(vec![ruling_json(ALPHA, "accept")], delete(0));
    if let Err(text) = validate_contract_rulings(&accepted, &disputed, &replaced, &contract_index) {
        panic!("a patch at the accepted contract's index is valid: {text}");
    }

    let rejected = verdict(vec![ruling_json(ALPHA, "reject")], delete(0));
    let result = validate_contract_rulings(&rejected, &disputed, &replaced, &contract_index);
    let text = error_text(result, "a patch at a rejected contract's index");
    assert!(text.contains("outside"), "refusal text: {text}");

    let undisputed = verdict(vec![ruling_json(ALPHA, "accept")], delete(1));
    let result = validate_contract_rulings(&undisputed, &disputed, &replaced, &contract_index);
    let text = error_text(result, "a patch at an undisputed contract's index");
    assert!(text.contains("outside"), "refusal text: {text}");
}

fn ruling(contract: &str, kind: ContractRulingKind) -> ContractRuling {
    ContractRuling {
        contract: contract.to_string(),
        ruling: kind,
        reasoning: format!("ruling on {contract}"),
        citations: vec![Citation {
            file: "loom/src/lib.rs".to_string(),
            line: None,
            excerpt: "pub mod verify;".to_string(),
            claim: "cited".to_string(),
        }],
    }
}

#[test]
fn a_contract_file_is_written_only_when_all_its_contracts_are_accepted() {
    let contract_files: BTreeMap<String, String> =
        [(ALPHA, "x.rs"), (BETA, "x.rs"), (GAMMA, "y.rs")]
            .iter()
            .map(|(id, file)| ((*id).to_string(), (*file).to_string()))
            .collect();
    let replacements = ids(&["x.rs", "y.rs", "h.txt"]);

    let all_accepted = [
        ruling(ALPHA, ContractRulingKind::Accept),
        ruling(BETA, ContractRulingKind::Accept),
        ruling(GAMMA, ContractRulingKind::Accept),
    ];
    assert_eq!(
        files_to_write(&all_accepted, &contract_files, &replacements),
        ids(&["x.rs", "y.rs", "h.txt"]),
        "every accept writes every replacement, in filing order"
    );

    let one_rejected = [
        ruling(ALPHA, ContractRulingKind::Accept),
        ruling(BETA, ContractRulingKind::Reject),
        ruling(GAMMA, ContractRulingKind::Accept),
    ];
    assert_eq!(
        files_to_write(&one_rejected, &contract_files, &replacements),
        ids(&["y.rs"]),
        "x.rs holds a rejected contract and any reject holds back the harness file"
    );
}
