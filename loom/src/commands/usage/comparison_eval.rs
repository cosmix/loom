use std::collections::BTreeSet;

use super::comparison_latency::{self, LatencySummary};
use super::comparison_net::{self, NetTotalSummary};
use super::comparison_quality;
use super::comparison_quota;
use super::comparison_schema::{
    ComparisonArtifact, ComparisonPair, ComparisonReport, Outcome, PairResult, ProviderAssignment,
    TokenRule, COMPARISON_SCHEMA_VERSION,
};
use super::comparison_token;
use super::comparison_validation;

/// One pair's verdict plus the pieces the declared rules recombine.
struct PairOutcome {
    result: PairResult,
    quality: Outcome,
    latency: Outcome,
    /// Structurally valid with measured, complete accounting on both runs.
    usable: bool,
}

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
    aggregate_report(&artifact, pairs)
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
        net_total: None,
        latency: None,
        claim_scope: "finite-fixture-set",
    }
}

fn evaluate_pair(
    artifact: &ComparisonArtifact,
    pair: &ComparisonPair,
    duplicate_ids: &BTreeSet<String>,
) -> PairOutcome {
    let mut reasons = comparison_validation::pair_reasons(artifact, pair, duplicate_ids);
    let structurally_valid = reasons.is_empty() && comparable(artifact, pair, &mut reasons);
    let quality = comparison_quality::evaluate(pair, &mut reasons);
    // A declared tolerance moves the latency judgement to the report level.
    let latency = if artifact.intervention.latency_tolerance_ms.is_some() {
        Outcome::SupportedCandidate
    } else {
        latency_verdict(pair, &mut reasons)
    };
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
    let result = PairResult {
        work_unit_id: safe_work_unit_id(&pair.work_unit_id),
        verdict,
        token_proxy_verdict: token_proxy,
        subscription_verdict: subscription,
        reason_codes: reasons,
        baseline_provider_tokens: pair.baseline.provider_tokens.clone(),
        candidate_provider_tokens: pair.candidate.provider_tokens.clone(),
        quota_comparisons: comparison_quota::evidence(&pair.baseline, &pair.candidate),
    };
    PairOutcome {
        result,
        quality,
        latency,
        usable: structurally_valid
            && comparison_token::complete_accounting(&pair.baseline)
            && comparison_token::complete_accounting(&pair.candidate),
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

fn aggregate_report(artifact: &ComparisonArtifact, outcomes: Vec<PairOutcome>) -> ComparisonReport {
    let mut reasons = outcomes
        .iter()
        .flat_map(|outcome| outcome.result.reason_codes.iter().copied())
        .collect::<Vec<_>>();
    let latency = artifact
        .intervention
        .latency_tolerance_ms
        .map(|tolerance| declared_latency(artifact, tolerance, &mut reasons));
    let net_total = (artifact.intervention.token_rule == TokenRule::NetTotal)
        .then(|| declared_net_total(artifact, &outcomes, &mut reasons));
    let verdict = overall_verdict(&outcomes, net_total.as_ref(), latency.as_ref());
    let token_proxy = aggregate(outcomes.iter().map(|o| o.result.token_proxy_verdict));
    let subscription = aggregate(outcomes.iter().map(|o| o.result.subscription_verdict));
    sort_reasons(&mut reasons);
    ComparisonReport {
        schema_version: COMPARISON_SCHEMA_VERSION,
        verdict,
        token_proxy_verdict: token_proxy,
        subscription_verdict: subscription,
        reason_codes: reasons,
        pairs: outcomes.into_iter().map(|o| o.result).collect(),
        net_total,
        latency,
        claim_scope: "finite-fixture-set",
    }
}

fn declared_latency(
    artifact: &ComparisonArtifact,
    tolerance_ms: u64,
    reasons: &mut Vec<&'static str>,
) -> LatencySummary {
    let all = artifact.pairs.iter().collect::<Vec<_>>();
    comparison_latency::evaluate(&all, tolerance_ms, reasons)
}

fn declared_net_total(
    artifact: &ComparisonArtifact,
    outcomes: &[PairOutcome],
    reasons: &mut Vec<&'static str>,
) -> NetTotalSummary {
    let usable = artifact
        .pairs
        .iter()
        .zip(outcomes)
        .filter_map(|(pair, outcome)| outcome.usable.then_some(pair))
        .collect::<Vec<_>>();
    let summary = comparison_net::evaluate(&usable, outcomes.len() - usable.len());
    reasons.extend(summary.reason_codes.iter().copied());
    summary
}

fn overall_verdict(
    outcomes: &[PairOutcome],
    net_total: Option<&NetTotalSummary>,
    latency: Option<&LatencySummary>,
) -> Outcome {
    let declared_latency = latency.map(|summary| summary.verdict);
    let Some(net_total) = net_total else {
        let pairs = aggregate(outcomes.iter().map(|o| o.result.verdict));
        return declared_latency.map_or(pairs, |latency| pairs.worse(latency));
    };
    let quality = aggregate(outcomes.iter().map(|o| o.quality));
    let latency = declared_latency
        .unwrap_or_else(|| aggregate(outcomes.iter().map(|outcome| outcome.latency)));
    // Quota is whole percents and cannot resolve a plausible saving, so an
    // inconclusive subscription verdict does not hold back net-total support.
    let subscription_rejected = outcomes
        .iter()
        .any(|o| o.result.subscription_verdict == Outcome::Rejected);
    let parts = [quality, latency, net_total.verdict];
    if subscription_rejected || parts.contains(&Outcome::Rejected) {
        Outcome::Rejected
    } else if parts
        .iter()
        .all(|part| *part == Outcome::SupportedCandidate)
    {
        Outcome::SupportedCandidate
    } else {
        Outcome::Inconclusive
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

#[cfg(test)]
#[path = "comparison_rules_tests.rs"]
mod rules_tests;
