//! Tests for verdict parsing/validation. Split out of `verdict.rs` to keep
//! that file under the maintainability limit (CLAUDE.md Rule 17); the module
//! path is unchanged.

use super::*;

#[test]
fn accept_with_citations_round_trips() {
    let raw = r#"{
        "verdict": "accept",
        "reasoning": "criterion was unreachable",
        "citations": [
            {"file": "src/a.rs", "line": 10, "excerpt": "fn foo()", "claim": "missing function"}
        ],
        "plan_patch": {"stage_id": "x", "field": "acceptance", "patch": {"op": "delete", "index": 1}}
    }"#;
    let out = parse_and_validate(raw);
    match out {
        ValidationOutcome::Verdict(DisputeVerdict::Accept {
            citations,
            reasoning,
            ..
        }) => {
            assert_eq!(citations.len(), 1);
            assert_eq!(reasoning, "criterion was unreachable");
        }
        other => panic!("expected Accept, got {other:?}"),
    }
}

#[test]
fn accept_without_citations_coerced_to_needs_more() {
    let raw = r#"{
        "verdict": "accept",
        "reasoning": "feels right",
        "citations": [],
        "plan_patch": {}
    }"#;
    match parse_and_validate(raw) {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
            assert!(questions[0].contains("citation"));
        }
        other => panic!("expected NeedsMoreEvidence, got {other:?}"),
    }
}

/// The wedge this module exists to fix: three adjudicators emitted a FLAT
/// `plan_patch` (`op`/`index`/`value` as siblings of `field`, not nested
/// under `patch`). It must still validate as Accept, and what lands on disk
/// must be the canonical nested shape so `apply.rs` never has to normalise a
/// second time.
#[test]
fn flat_plan_patch_yields_accept_with_canonical_stored_shape() {
    let raw = r#"{
        "verdict": "accept",
        "reasoning": "criterion was overspecified",
        "citations": [
            {"file": "src/a.rs", "line": 10, "excerpt": "fn foo()", "claim": "missing function"}
        ],
        "plan_patch": {
            "stage_id": "s1",
            "field": "acceptance",
            "op": "replace",
            "index": 2,
            "value": "loom knowledge check",
            "reason": "criterion was wrong"
        }
    }"#;
    match parse_and_validate(raw) {
        ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) => {
            assert_eq!(
                plan_patch.inner,
                serde_json::json!({
                    "field": "acceptance",
                    "patches": [{"op": "replace", "index": 2, "value": "loom knowledge check"}],
                    "reason": "criterion was wrong",
                }),
                "stored plan_patch must be the canonical nested shape: {:?}",
                plan_patch.inner
            );
        }
        other => panic!("expected Accept, got {other:?}"),
    }
}

/// A `plan_patch` malformed under BOTH accepted shapes must re-prompt rather
/// than wedge — this is what lets an already-broken verdict on disk self-heal
/// instead of retrying the same failure forever.
#[test]
fn plan_patch_malformed_under_both_shapes_needs_more_evidence() {
    let raw = r#"{
        "verdict": "accept",
        "reasoning": "criterion was overspecified",
        "citations": [
            {"file": "src/a.rs", "line": 10, "excerpt": "fn foo()", "claim": "missing function"}
        ],
        "plan_patch": {"field": "acceptance"}
    }"#;
    match parse_and_validate(raw) {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
            assert!(
                questions[0].contains("plan_patch"),
                "question must name plan_patch: {questions:?}"
            );
        }
        other => panic!("expected NeedsMoreEvidence, got {other:?}"),
    }
}

#[test]
fn reject_with_citations() {
    let raw = r#"{
        "verdict": "reject",
        "reasoning": "criterion is right",
        "citations": [{"file":"a","excerpt":"e","claim":"c"}]
    }"#;
    match parse_and_validate(raw) {
        ValidationOutcome::Verdict(DisputeVerdict::Reject { citations, .. }) => {
            assert_eq!(citations.len(), 1);
            assert!(citations[0].line.is_none());
        }
        other => panic!("expected Reject, got {other:?}"),
    }
}

#[test]
fn needs_more_evidence_with_questions() {
    let raw = r#"{"verdict": "needs-more-evidence", "questions": ["what is X?"]}"#;
    match parse_and_validate(raw) {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
            assert_eq!(questions, vec!["what is X?".to_string()]);
        }
        other => panic!("expected NeedsMoreEvidence, got {other:?}"),
    }
}

#[test]
fn needs_more_evidence_with_no_questions_escalates() {
    let raw = r#"{"verdict": "needs-more-evidence", "questions": []}"#;
    match parse_and_validate(raw) {
        ValidationOutcome::Escalate { reason } => {
            assert!(reason.contains("pathological"));
        }
        other => panic!("expected Escalate, got {other:?}"),
    }
}

#[test]
fn unknown_verdict_tag_coerces_to_needs_more() {
    let raw = r#"{"verdict": "bogus"}"#;
    match parse_and_validate(raw) {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
            assert!(questions[0].contains("unknown verdict tag"));
        }
        other => panic!("expected NeedsMoreEvidence, got {other:?}"),
    }
}

#[test]
fn malformed_json_coerces_to_needs_more() {
    let raw = "not json at all";
    match parse_and_validate(raw) {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
            assert!(
                questions[0].contains("not valid JSON"),
                "got: {:?}",
                questions
            );
        }
        other => panic!("expected NeedsMoreEvidence, got {other:?}"),
    }
}

#[test]
fn fenced_json_is_accepted() {
    let raw = "```json\n{\"verdict\":\"reject\",\"reasoning\":\"r\",\"citations\":[{\"file\":\"f\",\"excerpt\":\"e\",\"claim\":\"c\"}]}\n```";
    assert!(matches!(
        parse_and_validate(raw),
        ValidationOutcome::Verdict(DisputeVerdict::Reject { .. })
    ));
}

#[test]
fn json_with_leading_prose_extracts_first_object() {
    let raw = "Sure! Here is the verdict:\n{\"verdict\":\"reject\",\"reasoning\":\"r\",\"citations\":[{\"file\":\"f\",\"excerpt\":\"e\",\"claim\":\"c\"}]}\nLet me know if you want more.";
    assert!(matches!(
        parse_and_validate(raw),
        ValidationOutcome::Verdict(DisputeVerdict::Reject { .. })
    ));
}

/// An accept verdict whose `plan_patch` deletes entry 0 of `field`.
fn accept_amending(field: &str) -> String {
    serde_json::json!({
        "verdict": "accept",
        "reasoning": "the disputed entry cannot pass against a correct implementation",
        "citations": [
            {"file": "src/a.rs", "line": 1, "excerpt": "fn a()", "claim": "the entry is wrong"}
        ],
        "plan_patch": {"field": field, "patch": {"op": "delete", "index": 0}, "reason": "r"}
    })
    .to_string()
}

/// The `plan_patch.field` an accepted verdict stores; `None` for any other
/// outcome.
fn amended_field(outcome: ValidationOutcome) -> Option<String> {
    match outcome {
        ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) => plan_patch
            .inner
            .get("field")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

/// The question an outcome asks, or a panic naming the outcome.
fn the_question(outcome: ValidationOutcome) -> String {
    match outcome {
        ValidationOutcome::Verdict(DisputeVerdict::NeedsMoreEvidence { questions }) => {
            questions.join("\n")
        }
        other => panic!("expected NeedsMoreEvidence, got {other:?}"),
    }
}

#[test]
fn a_wiring_tests_accept_must_amend_wiring_tests() {
    let kind = DisputeKind::criterion(CriterionField::WiringTests, vec![0]);
    for spelling in ["wiring-tests", "wiring_tests"] {
        let outcome = parse_and_validate_for(&accept_amending(spelling), &kind);
        assert_eq!(amended_field(outcome).as_deref(), Some("wiring-tests"));
    }
    for field in ["acceptance", "wiring"] {
        let question = the_question(parse_and_validate_for(&accept_amending(field), &kind));
        assert!(question.contains("wiring-tests list"), "{question}");
        assert!(
            question.contains("\"field\": \"wiring-tests\""),
            "{question}"
        );
    }
}

#[test]
fn a_wiring_accept_may_not_amend_acceptance() {
    let kind = DisputeKind::criterion(CriterionField::Wiring, vec![1]);
    let question = the_question(parse_and_validate_for(
        &accept_amending("acceptance"),
        &kind,
    ));
    assert!(question.contains("wiring list"), "{question}");
    assert!(question.contains("\"field\": \"wiring\""), "{question}");
    let outcome = parse_and_validate_for(&accept_amending("wiring"), &kind);
    assert_eq!(amended_field(outcome).as_deref(), Some("wiring"));
}

#[test]
fn an_acceptance_accept_may_still_amend_wiring() {
    let kind = DisputeKind::criterion(CriterionField::Acceptance, vec![0]);
    for field in ["acceptance", "wiring"] {
        let outcome = parse_and_validate_for(&accept_amending(field), &kind);
        assert_eq!(amended_field(outcome).as_deref(), Some(field));
    }
    let question = the_question(parse_and_validate_for(
        &accept_amending("wiring-tests"),
        &kind,
    ));
    assert!(question.contains("acceptance list"), "{question}");
}

/// A malformed patch's re-emit hint names the field the dispute admits.
#[test]
fn a_malformed_patch_hint_names_the_disputed_field() {
    let raw = serde_json::json!({
        "verdict": "accept",
        "reasoning": "r",
        "citations": [{"file": "f", "excerpt": "e", "claim": "c"}],
        "plan_patch": {"field": "wiring-tests"}
    })
    .to_string();
    let kind = DisputeKind::criterion(CriterionField::WiringTests, vec![0]);
    let question = the_question(parse_and_validate_for(&raw, &kind));
    assert!(
        question.contains("{\"field\": \"wiring-tests\", \"patches\""),
        "{question}"
    );
}

/// An accept over criteria 13 and 14 carrying `patches`.
fn accept_patching(patches: Value) -> String {
    serde_json::json!({
        "verdict": "accept",
        "reasoning": "13 needs a portable form; 14 runs nowhere completion runs",
        "citations": [{"file": "PLAN.md", "line": 9, "excerpt": "curl", "claim": "network denied"}],
        "plan_patch": {"field": "acceptance", "patches": patches, "reason": "portable forms"}
    })
    .to_string()
}

#[test]
fn an_accept_may_patch_several_criteria_and_records_patches() {
    let kind = DisputeKind::criterion(CriterionField::Acceptance, vec![13, 14]);
    let raw = accept_patching(serde_json::json!([
        {"op": "replace", "index": 13, "value": "cargo test --lib"},
        {"op": "delete", "index": 14}
    ]));

    let ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) =
        parse_and_validate_for(&raw, &kind)
    else {
        panic!("expected Accept");
    };
    assert_eq!(plan_patch.inner["patches"][0]["index"], 13);
    assert_eq!(plan_patch.inner["patches"][1]["op"], "delete");
    assert!(plan_patch.inner.get("patch").is_none());
}

#[test]
fn an_accept_patching_one_index_twice_needs_more_evidence() {
    let kind = DisputeKind::criterion(CriterionField::Acceptance, vec![13, 14]);
    let raw = accept_patching(serde_json::json!([
        {"op": "replace", "index": 14, "value": "cargo test --lib"},
        {"op": "delete", "index": 14}
    ]));

    let question = the_question(parse_and_validate_for(&raw, &kind));

    assert!(question.contains("index 14 twice"), "{question}");
}
