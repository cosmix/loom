use std::path::PathBuf;

use clap::Parser;

use super::comparison_eval;
use super::comparison_schema::{ComparisonArtifact, Outcome};
use super::UsageArgs;

const VALID: &[u8] =
    include_bytes!("../../../tests/fixtures/token_optimization/comparison/valid_improvement.json");

#[derive(Debug, Parser)]
struct UsageHarness {
    #[command(flatten)]
    args: UsageArgs,
}

#[test]
fn comparison_defaults_do_not_conflict() {
    let parsed = UsageHarness::try_parse_from(["usage", "--compare", "fixture.json"])
        .expect("comparison should accept default selection values");

    assert_eq!(parsed.args.compare, Some(PathBuf::from("fixture.json")));
    assert_eq!(parsed.args.since(), "7d");
    assert_eq!(parsed.args.provider(), super::ProviderSelection::Claude);
    assert_eq!(
        parsed.args.windows(),
        super::accounting::Windowing::FiveHour
    );
    assert!(!parsed.args.has_explicit_selection());
}

#[test]
fn comparison_explicit_selection_flags_are_detected() {
    let cases = [
        vec!["--since", "1d"],
        vec!["--until", "2026-09-12T00:00:00Z"],
        vec!["--provider", "codex"],
        vec!["--claude-root", "scratch"],
        vec!["--codex-root", "scratch"],
        vec!["--receipts-root", "scratch"],
        vec!["--forward-receipts-root", "scratch"],
        vec!["--project", "scratch"],
        vec!["--all"],
        vec!["--stage", "stage-a"],
        vec!["--plan", "plan-a"],
        vec!["--windows", "week"],
    ];

    for explicit in cases {
        let args = [vec!["usage", "--compare", "fixture.json"], explicit].concat();
        let parsed = UsageHarness::try_parse_from(args)
            .expect("explicit selection flag should parse with comparison");
        assert!(
            parsed.args.has_explicit_selection(),
            "explicit selection flag must be detected"
        );
    }
}

#[test]
fn valid_improvement_supports_both_separate_verdicts() {
    let artifact: ComparisonArtifact = serde_json::from_slice(VALID).expect("valid fixture");

    let report = comparison_eval::evaluate(artifact);

    assert_eq!(
        (
            report.verdict,
            report.token_proxy_verdict,
            report.subscription_verdict,
            report.reason_codes,
        ),
        (
            Outcome::SupportedCandidate,
            Outcome::SupportedCandidate,
            Outcome::SupportedCandidate,
            Vec::new(),
        )
    );
}

#[test]
fn strict_schema_rejects_unknown_fields() {
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("valid fixture");
    value["pairs"][0]["candidate"]["raw_prompt"] = serde_json::json!("secret");

    let decoded = serde_json::from_value::<ComparisonArtifact>(value);

    assert!(decoded.is_err());
}

#[test]
fn quota_reset_crossing_is_inconclusive() {
    let mut value: serde_json::Value = serde_json::from_slice(VALID).expect("valid fixture");
    value["pairs"][0]["candidate"]["quota_observations"][0]["reset_id"] =
        serde_json::json!("reset-b");
    let artifact = serde_json::from_value(value).expect("mutated fixture remains valid");

    let report = comparison_eval::evaluate(artifact);

    assert_eq!(
        (
            report.verdict,
            report.subscription_verdict,
            report.reason_codes,
        ),
        (
            Outcome::Inconclusive,
            Outcome::Inconclusive,
            vec!["quota-reset-crossing"],
        )
    );
}
