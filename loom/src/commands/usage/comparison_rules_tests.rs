use serde_json::{json, Value};

use crate::commands::usage::comparison_eval;
use crate::commands::usage::comparison_schema::{ComparisonArtifact, ComparisonReport, Outcome};

const VALID: &str =
    include_str!("../../../tests/fixtures/token_optimization/comparison/valid_improvement.json");

fn fixture() -> Value {
    serde_json::from_str(VALID).expect("valid fixture")
}

fn evaluate(value: Value) -> ComparisonReport {
    let artifact: ComparisonArtifact = serde_json::from_value(value).expect("artifact decodes");
    comparison_eval::evaluate(artifact)
}

/// Four copies of the fixture pair whose candidate net totals sit 19, 22, 18
/// and 21 tokens below the baseline, with no quota observations.
fn net_total_fixture() -> Value {
    let mut value = fixture();
    let template = value["pairs"][0].clone();
    let reductions = [19, 22, 18, 21];
    let pairs = reductions
        .iter()
        .enumerate()
        .map(|(index, reduction)| {
            let mut pair = template.clone();
            pair["work_unit_id"] = json!(format!("unit-{index}"));
            pair["baseline"]["run_id"] = json!(format!("base-{index}"));
            pair["candidate"]["run_id"] = json!(format!("candidate-{index}"));
            for side in ["baseline", "candidate"] {
                pair[side]["quota_observations"] = Value::Null;
            }
            let tokens = &mut pair["candidate"]["provider_tokens"]["claude"];
            tokens["output_tokens"] = json!(40 - reduction);
            pair
        })
        .collect::<Vec<_>>();
    value["pairs"] = Value::Array(pairs);
    value["intervention"]["token_rule"] = json!("net-total");
    value
}

#[test]
fn undeclared_rules_leave_the_report_unchanged() {
    let undeclared = serde_json::to_value(evaluate(fixture())).expect("report serializes");
    let mut explicit = fixture();
    explicit["intervention"]["token_rule"] = json!("dominance");
    explicit["intervention"]["latency_tolerance_ms"] = Value::Null;
    let explicit = serde_json::to_value(evaluate(explicit)).expect("report serializes");

    assert_eq!(undeclared, explicit);
    assert!(undeclared.get("net_total").is_none());
    assert!(undeclared.get("latency").is_none());
    assert_eq!(undeclared["verdict"], json!("supported-candidate"));
}

#[test]
fn net_total_support_does_not_need_quota_observations() {
    let report = evaluate(net_total_fixture());

    assert_eq!(
        report.net_total.as_ref().map(|net| net.verdict),
        Some(Outcome::SupportedCandidate)
    );
    assert_eq!(report.subscription_verdict, Outcome::Inconclusive);
    assert_eq!(report.verdict, Outcome::SupportedCandidate);
}

#[test]
fn net_total_rejects_when_one_pair_review_failed() {
    let mut value = net_total_fixture();
    value["pairs"][0]["candidate"]["reviewed_dimensions"][0]["verdict"] = json!("failed");

    assert_eq!(evaluate(value).verdict, Outcome::Rejected);
}

#[test]
fn a_declared_latency_tolerance_accepts_a_small_slowdown() {
    let mut slower = fixture();
    slower["pairs"][0]["candidate"]["critical_path_ms"] = json!(1_001);
    let undeclared = evaluate(slower.clone());
    slower["intervention"]["latency_tolerance_ms"] = json!(5);
    let declared = evaluate(slower);

    assert_eq!(undeclared.verdict, Outcome::Rejected);
    assert_ne!(declared.verdict, Outcome::Rejected);
    assert!(declared.latency.is_some());
}

#[test]
fn one_second_resets_at_jitter_compares_and_301_seconds_crosses() {
    let mut jitter = fixture();
    jitter["pairs"][0]["candidate"]["quota_observations"][0]["resets_at"] = json!(2_001);
    let jitter = evaluate(jitter);
    let mut crossing = fixture();
    crossing["pairs"][0]["candidate"]["quota_observations"][0]["resets_at"] = json!(2_301);
    let crossing = evaluate(crossing);

    assert_eq!(jitter.subscription_verdict, Outcome::SupportedCandidate);
    assert!(jitter.reason_codes.is_empty());
    assert_eq!(crossing.subscription_verdict, Outcome::Inconclusive);
    assert!(crossing.reason_codes.contains(&"quota-reset-crossing"));
}

#[test]
fn observation_intervals_within_one_poll_compare() {
    let mut near = fixture();
    near["pairs"][0]["candidate"]["quota_observations"][0]["observed_end_at"] = json!(1_400);
    let near = evaluate(near);
    let mut far = fixture();
    far["pairs"][0]["candidate"]["quota_observations"][0]["observed_end_at"] = json!(1_481);
    let far = evaluate(far);

    assert!(!near.reason_codes.contains(&"quota-interval-mismatch"));
    assert_eq!(near.subscription_verdict, Outcome::SupportedCandidate);
    assert!(far.reason_codes.contains(&"quota-interval-mismatch"));
}

#[test]
fn a_changed_environment_makes_the_pair_incomparable() {
    let same = evaluate(fixture());
    let mut changed = fixture();
    changed["pairs"][0]["candidate"]["environment_id"] = json!("linux-arm-test");
    let changed = evaluate(changed);

    assert!(!same.reason_codes.contains(&"incomparable-environment"));
    assert!(changed.reason_codes.contains(&"incomparable-environment"));
}
