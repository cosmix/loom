use super::super::comparison_schema::{ComparisonArtifact, ComparisonPair, Outcome};
use super::evaluate;

const VALID: &str =
    include_str!("../../../tests/fixtures/token_optimization/comparison/valid_improvement.json");

fn fixture_pair() -> ComparisonPair {
    serde_json::from_str::<ComparisonArtifact>(VALID)
        .expect("valid fixture")
        .pairs
        .into_iter()
        .next()
        .expect("fixture pair")
}

fn pair_with_critical_increase(increase: i64) -> ComparisonPair {
    let mut pair = fixture_pair();
    let baseline = pair
        .baseline
        .critical_path_ms
        .expect("fixture has critical-path latency");
    pair.candidate.critical_path_ms = Some(shifted(baseline, increase));
    pair
}

fn shifted(value: u64, increase: i64) -> u64 {
    if increase.is_negative() {
        value
            .checked_sub(increase.unsigned_abs())
            .expect("test latency remains non-negative")
    } else {
        value
            .checked_add(increase as u64)
            .expect("test latency fits u64")
    }
}

#[test]
fn negative_mean_critical_path_increase_is_supported() {
    let slower = pair_with_critical_increase(1);
    let faster = pair_with_critical_increase(-3);
    let mut reasons = Vec::new();

    let summary = evaluate(&[&slower, &faster], 0, &mut reasons);

    assert_eq!(summary.critical_path.state, "judged");
    assert_eq!(summary.critical_path.pairs, 2);
    assert_eq!(summary.critical_path.mean_increase_ms, Some(-1.0));
    assert_eq!(summary.verdict, Outcome::SupportedCandidate);
    assert!(reasons.is_empty());
}

#[test]
fn positive_mean_critical_path_increase_is_rejected() {
    let first = pair_with_critical_increase(5);
    let second = pair_with_critical_increase(3);
    let mut reasons = Vec::new();

    let summary = evaluate(&[&first, &second], 0, &mut reasons);

    assert_eq!(summary.critical_path.mean_increase_ms, Some(4.0));
    assert_eq!(summary.critical_path.verdict, Outcome::Rejected);
    assert_eq!(summary.verdict, Outcome::Rejected);
    assert_eq!(reasons, ["critical-path-regression"]);
}

#[test]
fn tolerance_allows_mean_below_the_limit() {
    let first = pair_with_critical_increase(12);
    let second = pair_with_critical_increase(6);
    let mut reasons = Vec::new();

    let summary = evaluate(&[&first, &second], 10, &mut reasons);

    assert_eq!(summary.critical_path.mean_increase_ms, Some(9.0));
    assert_eq!(summary.critical_path.verdict, Outcome::SupportedCandidate);
    assert_eq!(summary.verdict, Outcome::SupportedCandidate);
    assert!(reasons.is_empty());
}

#[test]
fn unmeasured_notification_does_not_block_a_supported_critical_path() {
    let mut pair = fixture_pair();
    pair.baseline.notification_latency_ms = None;
    pair.candidate.notification_latency_ms = None;
    let mut reasons = Vec::new();

    let summary = evaluate(&[&pair], 0, &mut reasons);

    assert_eq!(summary.notification.state, "unmeasured");
    assert_eq!(summary.notification.pairs, 0);
    assert_eq!(summary.notification.verdict, Outcome::SupportedCandidate);
    assert_eq!(summary.verdict, Outcome::SupportedCandidate);
    assert_eq!(reasons, ["notification-latency-unmeasured"]);
}

#[test]
fn partial_notification_evidence_is_inconclusive() {
    let mut pair = fixture_pair();
    pair.candidate.notification_latency_ms = None;
    let mut reasons = Vec::new();

    let summary = evaluate(&[&pair], 0, &mut reasons);

    assert_eq!(summary.notification.state, "partial");
    assert_eq!(summary.notification.verdict, Outcome::Inconclusive);
    assert_eq!(summary.verdict, Outcome::Inconclusive);
    assert_eq!(reasons, ["missing-latency-evidence"]);
}
