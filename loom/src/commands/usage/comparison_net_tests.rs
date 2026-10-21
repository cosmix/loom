use serde_json::{json, Map, Value};

use super::super::comparison_schema::{ComparisonPair, Outcome};
use super::super::provider_types::Provider;
use super::evaluate;

const VALID: &str =
    include_str!("../../../tests/fixtures/token_optimization/comparison/valid_improvement.json");

#[test]
fn reductions_are_supported_per_provider() {
    let pairs = token_pairs(&[(1_000, 900), (1_000, 880), (1_000, 910), (1_000, 890)]);

    let summary = evaluate_pairs(&pairs);

    assert_eq!(summary.verdict, Outcome::SupportedCandidate);
    assert_eq!(summary.reason_codes, Vec::<&str>::new());
    assert_eq!(summary.providers[0].classification, "reduction");
}

#[test]
fn unchanged_totals_are_within_noise() {
    let pairs = token_pairs(&[(100, 100); 4]);

    let summary = evaluate_pairs(&pairs);

    assert_eq!(summary.verdict, Outcome::Inconclusive);
    assert_eq!(summary.reason_codes, vec!["net-total-within-noise"]);
    assert_eq!(summary.providers[0].classification, "within-noise");
}

#[test]
fn a_single_pair_is_insufficient() {
    let pairs = token_pairs(&[(100, 90)]);

    let summary = evaluate_pairs(&pairs);

    assert_eq!(summary.verdict, Outcome::Inconclusive);
    assert_eq!(summary.reason_codes, vec!["insufficient-pairs"]);
    assert_eq!(summary.providers[0].classification, "insufficient-pairs");
}

#[test]
fn opposite_provider_changes_are_inconclusive() {
    let pairs = (0..4)
        .map(|_| {
            pair(
                &[("claude", 1_000), ("codex", 1_000)],
                &[("claude", 900), ("codex", 1_100)],
            )
        })
        .collect::<Vec<_>>();

    let summary = evaluate_pairs(&pairs);

    assert_eq!(summary.verdict, Outcome::Inconclusive);
    assert_eq!(summary.reason_codes, vec!["cross-provider-transfer"]);
}

#[test]
fn rising_totals_are_rejected() {
    let pairs = token_pairs(&[(1_000, 1_100); 4]);

    let summary = evaluate_pairs(&pairs);

    assert_eq!(summary.verdict, Outcome::Rejected);
    assert_eq!(summary.reason_codes, vec!["net-total-regression"]);
    assert_eq!(summary.providers[0].classification, "regression");
}

#[test]
fn a_missing_net_dimension_is_inconclusive() {
    let mut pair = pair(&[("claude", 100)], &[("claude", 90)]);
    pair.candidate
        .provider_tokens
        .get_mut(&Provider::Claude)
        .expect("Claude vector")
        .output_tokens = None;

    let summary = evaluate_pairs(&[pair]);

    assert_eq!(summary.verdict, Outcome::Inconclusive);
    assert_eq!(summary.reason_codes, vec!["missing-token-dimension"]);
    assert_eq!(summary.providers[0].classification, "missing-dimension");
}

#[test]
fn known_reductions_report_sample_standard_error() {
    let pairs = token_pairs(&[(100, 90), (100, 80), (100, 70), (100, 60)]);

    let summary = evaluate_pairs(&pairs);
    let provider = &summary.providers[0];

    assert!((provider.mean_reduction.expect("mean") - 25.0).abs() < 1e-9);
    assert!(
        (provider.standard_error.expect("standard error") - (125.0 / 3.0_f64).sqrt()).abs() < 1e-9
    );
}

#[test]
fn exclusions_and_empty_evidence_are_inconclusive() {
    let pairs = token_pairs(&[(100, 90); 3]);
    let refs = pairs.iter().collect::<Vec<_>>();

    let excluded = evaluate(&refs, 1);
    let empty = evaluate(&[], 0);

    assert_eq!(excluded.reason_codes, vec!["net-total-excluded-pairs"]);
    assert_eq!(empty.reason_codes, vec!["insufficient-pairs"]);
}

#[test]
fn mismatched_provider_keys_are_inconclusive() {
    let pairs = vec![pair(&[("claude", 100)], &[("codex", 90)])];

    let summary = evaluate_pairs(&pairs);

    assert_eq!(summary.reason_codes, vec!["cross-provider-transfer"]);
    assert!(summary.providers.is_empty());
}

fn evaluate_pairs(pairs: &[ComparisonPair]) -> super::NetTotalSummary {
    let refs = pairs.iter().collect::<Vec<_>>();
    evaluate(&refs, 0)
}

fn token_pairs(values: &[(u64, u64)]) -> Vec<ComparisonPair> {
    values
        .iter()
        .map(|(baseline, candidate)| pair(&[("claude", *baseline)], &[("claude", *candidate)]))
        .collect()
}

fn pair(baseline: &[(&str, u64)], candidate: &[(&str, u64)]) -> ComparisonPair {
    let mut artifact: Value = serde_json::from_str(VALID).expect("valid fixture JSON");
    let mut value = artifact["pairs"][0].take();
    value["baseline"]["provider_tokens"] = vectors(baseline);
    value["candidate"]["provider_tokens"] = vectors(candidate);
    serde_json::from_value(value).expect("valid comparison pair")
}

fn vectors(values: &[(&str, u64)]) -> Value {
    let mut vectors = Map::new();
    for (provider, total) in values {
        vectors.insert(
            (*provider).to_owned(),
            json!({ "resident_input_tokens": total, "output_tokens": 0 }),
        );
    }
    Value::Object(vectors)
}
