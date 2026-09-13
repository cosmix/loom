use std::collections::BTreeSet;

use super::comparison_quality;
use super::comparison_quota;
use super::comparison_schema::{
    ComparisonArtifact, ComparisonPair, ComparisonReport, Outcome, PairResult, ProviderAssignment,
    COMPARISON_SCHEMA_VERSION,
};
use super::comparison_token;
use super::comparison_validation;

pub(super) fn evaluate(artifact: ComparisonArtifact) -> ComparisonReport {
    let artifact_reasons = comparison_validation::artifact_reasons(&artifact);
    if !artifact_reasons.is_empty() {
        return failure_report(artifact_reasons);
    }
    let duplicates = comparison_validation::duplicate_run_ids(&artifact);
    let pairs = artifact
        .pairs
        .iter()
        .map(|pair| evaluate_pair(&artifact, pair, &duplicates))
        .collect::<Vec<_>>();
    aggregate_report(pairs)
}

pub(super) fn failure_report(mut reasons: Vec<&'static str>) -> ComparisonReport {
    sort_reasons(&mut reasons);
    ComparisonReport {
        schema_version: COMPARISON_SCHEMA_VERSION,
        verdict: Outcome::Inconclusive,
        token_proxy_verdict: Outcome::Inconclusive,
        subscription_verdict: Outcome::Inconclusive,
        reason_codes: reasons,
        pairs: Vec::new(),
        claim_scope: "finite-fixture-set",
    }
}

fn evaluate_pair(
    artifact: &ComparisonArtifact,
    pair: &ComparisonPair,
    duplicate_ids: &BTreeSet<String>,
) -> PairResult {
    let mut reasons = comparison_validation::pair_reasons(artifact, pair, duplicate_ids);
    let structurally_valid = reasons.is_empty() && comparable(artifact, pair, &mut reasons);
    let quality = comparison_quality::evaluate(pair, &mut reasons);
    let latency = latency_verdict(pair, &mut reasons);
    let (token_proxy, subscription) = if structurally_valid {
        (
            comparison_token::evaluate(&pair.baseline, &pair.candidate, &mut reasons),
            comparison_quota::evaluate(&pair.baseline, &pair.candidate, &mut reasons),
        )
    } else {
        reasons.push("incomparable-pair");
        (Outcome::Inconclusive, Outcome::Inconclusive)
    };
    let verdict = overall_pair(structurally_valid, quality, latency, subscription);
    sort_reasons(&mut reasons);
    PairResult {
        work_unit_id: safe_work_unit_id(&pair.work_unit_id),
        verdict,
        token_proxy_verdict: token_proxy,
        subscription_verdict: subscription,
        reason_codes: reasons,
        baseline_provider_tokens: pair.baseline.provider_tokens.clone(),
        candidate_provider_tokens: pair.candidate.provider_tokens.clone(),
        quota_comparisons: comparison_quota::evidence(&pair.baseline, &pair.candidate),
    }
}

fn safe_work_unit_id(work_unit_id: &str) -> String {
    if comparison_validation::valid_id(work_unit_id) {
        work_unit_id.to_owned()
    } else {
        "invalid-work-unit".to_owned()
    }
}

fn comparable(
    artifact: &ComparisonArtifact,
    pair: &ComparisonPair,
    reasons: &mut Vec<&'static str>,
) -> bool {
    let baseline = &pair.baseline;
    let candidate = &pair.candidate;
    compare_identity(
        baseline.workload_id == candidate.workload_id,
        "incomparable-workload",
        reasons,
    );
    compare_identity(
        baseline.fixture_id == candidate.fixture_id
            && baseline.source_input_revision == candidate.source_input_revision
            && baseline.source_input_digest == candidate.source_input_digest,
        "incomparable-source-input",
        reasons,
    );
    compare_identity(
        baseline.environment_id == candidate.environment_id,
        "incomparable-environment",
        reasons,
    );
    compare_identity(
        baseline.acceptance_contract_digest == candidate.acceptance_contract_digest,
        "incomparable-acceptance-contract",
        reasons,
    );
    compare_identity(
        assignments_comparable(artifact, &baseline.assignments, &candidate.assignments),
        "model-effort-mismatch",
        reasons,
    );
    reasons.is_empty()
}

fn assignments_comparable(
    artifact: &ComparisonArtifact,
    baseline: &[ProviderAssignment],
    candidate: &[ProviderAssignment],
) -> bool {
    use super::comparison_schema::InterventionDimension as Change;

    if baseline.len() != candidate.len() {
        return false;
    }
    let allowed = &artifact.intervention.changed_dimensions;
    let providers_match = dimensions(baseline, |assignment| assignment.provider)
        == dimensions(candidate, |assignment| assignment.provider)
        || allowed.contains(&Change::ProviderAssignment);
    let models_match = dimensions(baseline, |assignment| assignment.model.clone())
        == dimensions(candidate, |assignment| assignment.model.clone())
        || allowed.contains(&Change::ModelAssignment);
    let efforts_match = dimensions(baseline, |assignment| assignment.effort.clone())
        == dimensions(candidate, |assignment| assignment.effort.clone())
        || allowed.contains(&Change::EffortAssignment);
    providers_match && models_match && efforts_match
}

fn dimensions<T: Ord>(
    assignments: &[ProviderAssignment],
    value: impl Fn(&ProviderAssignment) -> T,
) -> Vec<T> {
    let mut values = assignments.iter().map(value).collect::<Vec<_>>();
    values.sort();
    values
}

fn compare_identity(equal: bool, reason: &'static str, reasons: &mut Vec<&'static str>) {
    if !equal {
        reasons.push(reason);
    }
}

fn latency_verdict(pair: &ComparisonPair, reasons: &mut Vec<&'static str>) -> Outcome {
    let mut verdict = compare_latency(
        pair.baseline.critical_path_ms,
        pair.candidate.critical_path_ms,
        "critical-path-regression",
        reasons,
    );
    verdict = verdict.worse(compare_latency(
        pair.baseline.notification_latency_ms,
        pair.candidate.notification_latency_ms,
        "notification-latency-regression",
        reasons,
    ));
    verdict
}

fn compare_latency(
    baseline: Option<u64>,
    candidate: Option<u64>,
    regression_reason: &'static str,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    match baseline.zip(candidate) {
        Some((baseline, candidate)) if candidate > baseline => {
            reasons.push(regression_reason);
            Outcome::Rejected
        }
        Some(_) => Outcome::SupportedCandidate,
        None => {
            reasons.push("missing-latency-evidence");
            Outcome::Inconclusive
        }
    }
}

fn overall_pair(
    structurally_valid: bool,
    quality: Outcome,
    latency: Outcome,
    subscription: Outcome,
) -> Outcome {
    if [quality, latency, subscription].contains(&Outcome::Rejected) {
        Outcome::Rejected
    } else if structurally_valid
        && [quality, latency, subscription]
            .iter()
            .all(|verdict| *verdict == Outcome::SupportedCandidate)
    {
        Outcome::SupportedCandidate
    } else {
        Outcome::Inconclusive
    }
}

fn aggregate_report(pairs: Vec<PairResult>) -> ComparisonReport {
    let verdict = aggregate(pairs.iter().map(|pair| pair.verdict));
    let token_proxy = aggregate(pairs.iter().map(|pair| pair.token_proxy_verdict));
    let subscription = aggregate(pairs.iter().map(|pair| pair.subscription_verdict));
    let mut reasons = pairs
        .iter()
        .flat_map(|pair| pair.reason_codes.iter().copied())
        .collect::<Vec<_>>();
    sort_reasons(&mut reasons);
    ComparisonReport {
        schema_version: COMPARISON_SCHEMA_VERSION,
        verdict,
        token_proxy_verdict: token_proxy,
        subscription_verdict: subscription,
        reason_codes: reasons,
        pairs,
        claim_scope: "finite-fixture-set",
    }
}

fn aggregate(mut verdicts: impl Iterator<Item = Outcome>) -> Outcome {
    let Some(first) = verdicts.next() else {
        return Outcome::Inconclusive;
    };
    let mut rejected = first == Outcome::Rejected;
    let mut supported = first == Outcome::SupportedCandidate;
    for verdict in verdicts {
        rejected |= verdict == Outcome::Rejected;
        supported &= verdict == Outcome::SupportedCandidate;
    }
    if rejected {
        Outcome::Rejected
    } else if supported {
        Outcome::SupportedCandidate
    } else {
        Outcome::Inconclusive
    }
}

fn sort_reasons(reasons: &mut Vec<&'static str>) {
    reasons.sort_unstable();
    reasons.dedup();
}
