use super::*;
use crate::orchestrator::adjudication::verdict::parse_and_validate_for as parse_and_validate;
use serde_json::json;

fn findings(ids: &[&str]) -> DisputeKind {
    DisputeKind::Findings {
        finding_ids: ids.iter().map(|id| id.to_string()).collect(),
        evidence: Vec::new(),
    }
}

fn contract() -> DisputeKind {
    DisputeKind::Contract {
        contract_id: "rejects-x".to_string(),
    }
}

fn citation() -> Value {
    json!([{"file": "src/a.rs", "line": 3, "excerpt": "x", "claim": "y"}])
}

fn question(outcome: ValidationOutcome) -> String {
    match outcome {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
            questions.join(" ")
        }
        other => panic!("expected a coerced needs-more-evidence, got {other:?}"),
    }
}

#[test]
fn one_ruling_per_disputed_finding_is_recorded() {
    let raw = json!({"verdict": "rulings", "rulings": [
        {"finding": "F-1-1", "ruling": "uphold", "reasoning": "real bug"},
        {"finding": "F-1-2", "ruling": "defer", "target_stage": "s2", "reasoning": "later",
         "citations": citation()},
    ]});
    let rulings = match parse_and_validate(&raw.to_string(), &findings(&["F-1-1", "F-1-2"])) {
        ValidationOutcome::Verdict(DisputeVerdict::Rulings { rulings }) => rulings,
        other => panic!("expected rulings, got {other:?}"),
    };
    assert_eq!(rulings.len(), 2);
    assert_eq!(rulings[0].ruling, RulingKind::Uphold);
    assert_eq!(rulings[1].target_stage.as_deref(), Some("s2"));
}

#[test]
fn a_stray_or_missing_ruling_is_turned_into_a_question() {
    let stray = json!({"verdict": "rulings", "rulings": [
        {"finding": "F-9-9", "ruling": "uphold", "reasoning": "r"},
    ]});
    let asked = question(parse_and_validate(
        &stray.to_string(),
        &findings(&["F-1-1"]),
    ));
    assert!(asked.contains("F-9-9"), "{asked}");

    let missing = json!({"verdict": "rulings", "rulings": []});
    let asked = question(parse_and_validate(
        &missing.to_string(),
        &findings(&["F-1-1"]),
    ));
    assert!(
        asked.contains("F-1-1") && asked.contains("exactly one"),
        "{asked}"
    );
}

#[test]
fn a_closing_ruling_needs_a_citation_and_a_deferral_a_target() {
    let uncited = json!({"verdict": "rulings", "rulings": [
        {"finding": "F-1-1", "ruling": "dismiss", "reasoning": "r"},
    ]});
    let asked = question(parse_and_validate(
        &uncited.to_string(),
        &findings(&["F-1-1"]),
    ));
    assert!(asked.contains("citation"), "{asked}");

    let untargeted = json!({"verdict": "rulings", "rulings": [
        {"finding": "F-1-1", "ruling": "defer", "reasoning": "r", "citations": citation()},
    ]});
    let asked = question(parse_and_validate(
        &untargeted.to_string(),
        &findings(&["F-1-1"]),
    ));
    assert!(asked.contains("target_stage"), "{asked}");
}

#[test]
fn a_contract_accept_takes_only_a_contracts_patch() {
    let bare = json!({"verdict": "accept", "reasoning": "r", "citations": citation()});
    let ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) =
        parse_and_validate(&bare.to_string(), &contract())
    else {
        panic!("a contract accept without a plan_patch is valid");
    };
    assert_eq!(plan_patch.inner, json!({}));

    let flat = json!({"verdict": "accept", "reasoning": "r", "citations": citation(),
        "plan_patch": {"field": "contracts", "op": "delete", "index": 0}});
    let ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) =
        parse_and_validate(&flat.to_string(), &contract())
    else {
        panic!("a contracts patch is valid");
    };
    assert_eq!(plan_patch.inner["field"], "contracts");
    assert_eq!(plan_patch.inner["patch"]["op"], "delete");

    let wrong_field = json!({"verdict": "accept", "reasoning": "r", "citations": citation(),
        "plan_patch": {"field": "acceptance", "op": "delete", "index": 0}});
    let asked = question(parse_and_validate(&wrong_field.to_string(), &contract()));
    assert!(asked.contains("must be contracts"), "{asked}");
}

#[test]
fn a_verdict_foreign_to_the_kind_is_turned_into_a_question() {
    let rulings = json!({"verdict": "rulings", "rulings": []});
    let asked = question(parse_and_validate(&rulings.to_string(), &contract()));
    assert!(asked.contains("contract dispute"), "{asked}");

    let accept = json!({"verdict": "accept", "reasoning": "r", "citations": citation()});
    let asked = question(parse_and_validate(
        &accept.to_string(),
        &findings(&["F-1-1"]),
    ));
    assert!(asked.contains("findings dispute"), "{asked}");
}
