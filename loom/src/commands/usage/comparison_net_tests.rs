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

#[test]
fn a_mean_inside_twice_the_standard_error_is_within_noise() {
    // Reductions +50, -40, +45, -35: mean 5, sample variance 7250 / 3, so the
    // standard error is sqrt(7250 / 12), about 24.6.
    let pairs = token_pairs(&[(1_000, 950), (1_000, 1_040), (1_000, 955), (1_000, 1_035)]);

    let summary = evaluate_pairs(&pairs);
    let provider = &summary.providers[0];

    assert!((provider.mean_reduction.expect("mean") - 5.0).abs() < 1e-9);
    assert!(
        (provider.standard_error.expect("standard error") - (7_250.0_f64 / 12.0).sqrt()).abs()
            < 1e-9
    );
    assert_eq!(provider.classification, "within-noise");
    assert_eq!(summary.verdict, Outcome::Inconclusive);
    assert_eq!(summary.reason_codes, vec!["net-total-within-noise"]);
}

#[test]
fn a_mean_between_one_and_two_standard_errors_is_within_noise_in_both_directions() {
    // Reductions +30, -10, +40, 0: mean 15, standard error about 11.9, so the
    // mean is 1.26 standard errors out: a 1 SE gate would call it a reduction.
    let reduced = token_pairs(&[(1_000, 970), (1_000, 1_010), (1_000, 960), (1_000, 1_000)]);
    let raised = token_pairs(&[(1_000, 1_030), (1_000, 990), (1_000, 1_040), (1_000, 1_000)]);

    for pairs in [reduced, raised] {
        let summary = evaluate_pairs(&pairs);

        assert_eq!(summary.providers[0].classification, "within-noise");
        assert_eq!(summary.verdict, Outcome::Inconclusive);
    }
}

#[test]
fn two_usable_pairs_are_insufficient_even_when_they_agree() {
    let pairs = token_pairs(&[(1_000, 900), (1_000, 900)]);

    let summary = evaluate_pairs(&pairs);
    let provider = &summary.providers[0];

    assert_eq!(provider.pairs, 2);
    assert_eq!(provider.classification, "insufficient-pairs");
    assert_eq!(provider.mean_reduction, None);
    assert_eq!(provider.standard_error, None);
    assert_eq!(summary.verdict, Outcome::Inconclusive);
    assert_eq!(summary.reason_codes, vec!["insufficient-pairs"]);
}

#[test]
fn three_tight_reduction_pairs_are_evaluated() {
    // Reductions 100, 110, 90: mean 100, sample variance 100, standard error
    // sqrt(100 / 3).
    let pairs = token_pairs(&[(1_000, 900), (1_000, 890), (1_000, 910)]);

    let summary = evaluate_pairs(&pairs);
    let provider = &summary.providers[0];

    assert_eq!(provider.pairs, 3);
    assert!((provider.mean_reduction.expect("mean") - 100.0).abs() < 1e-9);
    assert!(
        (provider.standard_error.expect("standard error") - (100.0_f64 / 3.0).sqrt()).abs() < 1e-9
    );
    assert_eq!(provider.classification, "reduction");
    assert_eq!(summary.verdict, Outcome::SupportedCandidate);
    assert_eq!(summary.reason_codes, Vec::<&str>::new());
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
