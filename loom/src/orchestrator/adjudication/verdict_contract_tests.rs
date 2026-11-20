//! A contract dispute's verdict checked when it is recorded
//! (`verdict::parse_and_validate_in` -> `verdict_contract`): against the
//! stage's contract list and the freeze record the replacements name.

use serde_json::{json, Value};

use crate::models::dispute::{ContractRulingKind, DisputeKind, DisputeVerdict, FileReplacement};
use crate::orchestrator::adjudication::apply_contract_tests::fixture_of;
use crate::orchestrator::adjudication::verdict::{
    parse_and_validate_in, RecordContext, ValidationOutcome,
};
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::test_support::{contract, CONTRACT_FILE};
use crate::verify::transitions::load_stage;

const Y_FILE: &str = "tests/y_contract.rs";
const HARNESS_FILE: &str = "tests/support/helper.rs";

fn spec(id: &str, file: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        ..contract()
    }
}

/// A dispute over `alpha` (in [`CONTRACT_FILE`], replaced) and `gamma` (in
/// [`Y_FILE`], not replaced).
fn kind() -> DisputeKind {
    replacing(&["alpha", "gamma"], CONTRACT_FILE)
}

fn ruling(contract: &str, ruling: &str) -> Value {
    json!({"contract": contract, "ruling": ruling, "reasoning": "judged",
           "citations": [{"file": "src/a.rs", "excerpt": "x", "claim": "y"}]})
}

/// `raw` validated as recording would, against a stage holding `alpha` at
/// index 0 and `gamma` at index 1.
fn record(raw: &Value) -> Result<ValidationOutcome, String> {
    record_on(raw, &kind())
}

/// [`record`] for the dispute `kind`.
fn record_on(raw: &Value, kind: &DisputeKind) -> Result<ValidationOutcome, String> {
    let specs = vec![spec("alpha", CONTRACT_FILE), spec("gamma", Y_FILE)];
    record_in(raw, kind, specs)
}

/// [`record_on`] against a stage holding `specs`.
fn record_in(
    raw: &Value,
    kind: &DisputeKind,
    specs: Vec<ContractSpec>,
) -> Result<ValidationOutcome, String> {
    let fx = fixture_of(
        Some("."),
        specs,
        &[
            (Y_FILE, b"#[test]\nfn y() {}\n"),
            (HARNESS_FILE, b"pub fn ready() -> bool { false }\n"),
        ],
    );
    let stage = load_stage("s1", &fx.work).unwrap();
    let context = RecordContext {
        work_dir: &fx.work,
        stage: &stage,
    };
    parse_and_validate_in(&raw.to_string(), kind, &context)
}

#[test]
fn rulings_with_a_replacement_and_a_patch_are_recorded_canonically() {
    let raw = json!({"verdict": "contract-rulings",
        "rulings": [ruling("alpha", "accept"), ruling("gamma", "accept")],
        "plan_patch": {"field": "contracts", "op": "delete", "index": 1}});

    let Ok(ValidationOutcome::Verdict(DisputeVerdict::ContractRulings {
        rulings,
        plan_patch,
    })) = record(&raw)
    else {
        panic!("a replaced accept and a patched accept are valid");
    };

    assert_eq!(rulings.len(), 2);
    assert!(rulings
        .iter()
        .all(|r| r.ruling == ContractRulingKind::Accept));
    assert_eq!(plan_patch.inner["field"], "contracts");
    assert_eq!(plan_patch.inner["patches"][0]["op"], "delete");
}

#[test]
fn an_accept_whose_file_is_not_replaced_and_unpatched_is_refused() {
    let raw = json!({"verdict": "contract-rulings",
        "rulings": [ruling("alpha", "accept"), ruling("gamma", "accept")]});

    let problem = record(&raw).unwrap_err();

    assert!(problem.contains("changes nothing"), "{problem}");
    assert!(problem.contains("gamma"), "{problem}");
}

#[test]
fn a_plain_accept_on_a_contract_dispute_is_refused_naming_the_rulings_shape() {
    let raw = json!({"verdict": "accept", "reasoning": "r",
        "citations": [{"file": "src/a.rs", "excerpt": "x", "claim": "y"}]});

    let problem = record(&raw).unwrap_err();

    assert!(problem.contains("\"contract-rulings\""), "{problem}");
}

#[test]
fn a_missing_or_stray_ruling_is_refused() {
    let missing = json!({"verdict": "contract-rulings", "rulings": [ruling("alpha", "accept")]});
    let problem = record(&missing).unwrap_err();
    assert!(problem.contains("'gamma' has 0 rulings"), "{problem}");

    let stray = json!({"verdict": "contract-rulings", "rulings": [
        ruling("alpha", "reject"), ruling("gamma", "reject"), ruling("delta", "reject")]});
    let problem = record(&stray).unwrap_err();
    assert!(problem.contains("'delta'"), "{problem}");
}

#[test]
fn needs_more_evidence_is_recorded_as_before() {
    let raw = json!({"verdict": "needs-more-evidence", "questions": ["which scenario?"]});

    assert!(matches!(
        record(&raw),
        Ok(ValidationOutcome::Verdict(
            DisputeVerdict::NeedsMoreEvidence { .. }
        ))
    ));
}

#[test]
fn a_harness_replacement_lets_every_disputed_contract_be_accepted() {
    let harness = replacing(&["gamma"], HARNESS_FILE);
    let raw = json!({"verdict": "contract-rulings", "rulings": [ruling("gamma", "accept")]});

    let Ok(ValidationOutcome::Verdict(DisputeVerdict::ContractRulings { plan_patch, .. })) =
        record_on(&raw, &harness)
    else {
        panic!("an accept that lets the harness replacement be written changes something");
    };
    assert_eq!(
        plan_patch.inner,
        json!({}),
        "no plan_patch is recorded as {{}}"
    );
}

/// A dispute over `ids` replacing `file`.
fn replacing(ids: &[&str], file: &str) -> DisputeKind {
    DisputeKind::Contract {
        contract_ids: ids.iter().map(|id| (*id).to_string()).collect(),
        replacements: vec![FileReplacement {
            file: file.to_string(),
            content: "// replaced\n".to_string(),
            base_sha256: None,
        }],
    }
}

#[test]
fn an_accept_whose_file_another_reject_holds_back_is_refused() {
    let specs = vec![spec("alpha", CONTRACT_FILE), spec("beta", CONTRACT_FILE)];
    let kind = replacing(&["alpha", "beta"], CONTRACT_FILE);
    let raw = json!({"verdict": "contract-rulings",
        "rulings": [ruling("alpha", "accept"), ruling("beta", "reject")]});

    let problem = record_in(&raw, &kind, specs).unwrap_err();

    assert!(problem.contains("changes nothing"), "{problem}");
    assert!(problem.contains("'alpha'"), "{problem}");
    assert!(
        problem.contains("accept every contract of its file"),
        "{problem}"
    );
}

#[test]
fn an_accept_whose_only_change_is_a_held_back_harness_file_is_refused() {
    let kind = replacing(&["alpha", "gamma"], HARNESS_FILE);
    let raw = json!({"verdict": "contract-rulings",
        "rulings": [ruling("alpha", "reject"), ruling("gamma", "accept")]});

    let problem = record_on(&raw, &kind).unwrap_err();

    assert!(problem.contains("changes nothing"), "{problem}");
    assert!(problem.contains("'gamma'"), "{problem}");
}

#[test]
fn a_plan_patch_on_another_field_is_refused() {
    let raw = json!({"verdict": "contract-rulings",
        "rulings": [ruling("alpha", "accept"), ruling("gamma", "reject")],
        "plan_patch": {"field": "acceptance", "op": "delete", "index": 0}});

    let problem = record(&raw).unwrap_err();

    assert!(problem.contains("must be contracts"), "{problem}");
}

#[test]
fn deleting_an_accepted_contract_whose_file_is_replaced_is_refused() {
    let raw = json!({"verdict": "contract-rulings",
        "rulings": [ruling("alpha", "accept"), ruling("gamma", "reject")],
        "plan_patch": {"field": "contracts", "op": "delete", "index": 0}});

    let problem = record(&raw).unwrap_err();

    assert!(problem.contains("contract 'alpha'"), "{problem}");
    assert!(problem.contains("reject it"), "{problem}");
}
