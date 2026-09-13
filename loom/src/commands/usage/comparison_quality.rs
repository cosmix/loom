use std::collections::BTreeMap;

use super::comparison_schema::{
    CheckEvidence, ComparisonPair, EvidenceVerdict, FindingSeverity, Outcome, ReviewEvidence,
    RunEvidence,
};

pub(super) fn evaluate(pair: &ComparisonPair, reasons: &mut Vec<&'static str>) -> Outcome {
    let mut verdict = candidate_completion(&pair.candidate, reasons);
    verdict = verdict.worse(checks_verdict(pair, reasons));
    verdict = verdict.worse(reviews_verdict(pair, reasons));
    verdict = verdict.worse(findings_verdict(pair, reasons));
    if !pair.candidate.omitted_requirements.is_empty() {
        reasons.push("omitted-requirement");
        verdict = Outcome::Rejected;
    }
    if !baseline_complete(&pair.baseline) {
        reasons.push("baseline-quality-incomplete");
        verdict = verdict.worse(Outcome::Inconclusive);
    }
    verdict
}

fn baseline_complete(run: &RunEvidence) -> bool {
    run.accepted == Some(true)
        && run.semantic_completion == EvidenceVerdict::Passed
        && !run.required_checks.is_empty()
        && run
            .required_checks
            .iter()
            .all(|check| check.verdict == EvidenceVerdict::Passed)
        && !run.reviewed_dimensions.is_empty()
        && run
            .reviewed_dimensions
            .iter()
            .all(|review| review.independent && review.verdict == EvidenceVerdict::Passed)
        && run.omitted_requirements.is_empty()
}

fn candidate_completion(run: &RunEvidence, reasons: &mut Vec<&'static str>) -> Outcome {
    match (run.accepted, run.semantic_completion) {
        (Some(false), _) => {
            reasons.push("candidate-not-accepted");
            Outcome::Rejected
        }
        (_, EvidenceVerdict::Failed) => {
            reasons.push("semantic-completion-failed");
            Outcome::Rejected
        }
        (None, _) | (_, EvidenceVerdict::Unknown) => {
            reasons.push("missing-quality-evidence");
            Outcome::Inconclusive
        }
        (Some(true), EvidenceVerdict::Passed) => Outcome::SupportedCandidate,
    }
}

fn checks_verdict(pair: &ComparisonPair, reasons: &mut Vec<&'static str>) -> Outcome {
    let required = check_map(&pair.baseline.required_checks);
    let candidate = check_map(&pair.candidate.required_checks);
    if required.keys().any(|key| !candidate.contains_key(key)) {
        reasons.push("weakened-verification");
        return Outcome::Rejected;
    }
    evidence_verdict(
        candidate.values().copied(),
        "failed-required-check",
        reasons,
    )
}

fn check_map(checks: &[CheckEvidence]) -> BTreeMap<(&str, &str), EvidenceVerdict> {
    checks
        .iter()
        .map(|check| {
            (
                (check.command_id.as_str(), check.contract_id.as_str()),
                check.verdict,
            )
        })
        .collect()
}

fn reviews_verdict(pair: &ComparisonPair, reasons: &mut Vec<&'static str>) -> Outcome {
    let required = review_map(&pair.baseline.reviewed_dimensions);
    let candidate = review_map(&pair.candidate.reviewed_dimensions);
    if required.keys().any(|key| !candidate.contains_key(key))
        || candidate.values().any(|review| !review.independent)
    {
        reasons.push("weakened-review");
        return Outcome::Rejected;
    }
    evidence_verdict(
        candidate.values().map(|review| review.verdict),
        "failed-review",
        reasons,
    )
}

fn review_map(reviews: &[ReviewEvidence]) -> BTreeMap<&str, &ReviewEvidence> {
    reviews
        .iter()
        .map(|review| (review.dimension.as_str(), review))
        .collect()
}

fn evidence_verdict(
    mut verdicts: impl Iterator<Item = EvidenceVerdict>,
    failed_reason: &'static str,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    let Some(first) = verdicts.next() else {
        reasons.push("missing-quality-evidence");
        return Outcome::Inconclusive;
    };
    let mut failed = first == EvidenceVerdict::Failed;
    let mut unknown = first == EvidenceVerdict::Unknown;
    for verdict in verdicts {
        failed |= verdict == EvidenceVerdict::Failed;
        unknown |= verdict == EvidenceVerdict::Unknown;
    }
    if failed {
        reasons.push(failed_reason);
        Outcome::Rejected
    } else if unknown {
        reasons.push("missing-quality-evidence");
        Outcome::Inconclusive
    } else {
        Outcome::SupportedCandidate
    }
}

fn findings_verdict(pair: &ComparisonPair, reasons: &mut Vec<&'static str>) -> Outcome {
    let baseline = finding_map(&pair.baseline);
    for finding in &pair.candidate.unresolved_findings {
        match baseline.get(finding.finding_id.as_str()) {
            None => {
                reasons.push("new-unresolved-finding");
                return Outcome::Rejected;
            }
            Some(severity) if finding.severity > *severity => {
                reasons.push("worsened-unresolved-finding");
                return Outcome::Rejected;
            }
            _ => {}
        }
    }
    Outcome::SupportedCandidate
}

fn finding_map(run: &RunEvidence) -> BTreeMap<&str, FindingSeverity> {
    run.unresolved_findings
        .iter()
        .map(|finding| (finding.finding_id.as_str(), finding.severity))
        .collect()
}
