use super::comparison_schema::{AccountingProvenance, Outcome, RunEvidence};
use super::provider_types::{Provider, ProviderTokenVector};

pub(super) fn evaluate(
    baseline: &RunEvidence,
    candidate: &RunEvidence,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    if !complete_accounting(baseline) || !complete_accounting(candidate) {
        reasons.push("missing-accounting-evidence");
        return Outcome::Inconclusive;
    }
    if baseline.provider_tokens.is_empty() || candidate.provider_tokens.is_empty() {
        reasons.push("missing-provider-token-vector");
        return Outcome::Inconclusive;
    }
    if baseline
        .provider_tokens
        .keys()
        .ne(candidate.provider_tokens.keys())
    {
        reasons.push("cross-provider-transfer");
        return Outcome::Inconclusive;
    }
    compare_vectors(baseline, candidate, reasons)
}

fn complete_accounting(run: &RunEvidence) -> bool {
    run.accounting_complete && run.accounting_provenance == AccountingProvenance::Measured
}

fn compare_vectors(
    baseline: &RunEvidence,
    candidate: &RunEvidence,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    let mut reduced = false;
    let mut increased = false;
    for (provider, baseline_vector) in &baseline.provider_tokens {
        let candidate_vector = &candidate.provider_tokens[provider];
        let Some(changes) = dimension_changes(*provider, baseline_vector, candidate_vector) else {
            reasons.push("missing-token-dimension");
            return Outcome::Inconclusive;
        };
        reduced |= changes.0;
        increased |= changes.1;
    }
    classify_changes(reduced, increased, reasons)
}

fn dimension_changes(
    provider: Provider,
    baseline: &ProviderTokenVector,
    candidate: &ProviderTokenVector,
) -> Option<(bool, bool)> {
    let baseline = dimensions(provider, baseline);
    let candidate = dimensions(provider, candidate);
    let mut reduced = false;
    let mut increased = false;
    for (left, right) in baseline.into_iter().zip(candidate) {
        let (left, right) = (left?, right?);
        reduced |= right < left;
        increased |= right > left;
    }
    Some((reduced, increased))
}

fn dimensions(provider: Provider, vector: &ProviderTokenVector) -> Vec<Option<u64>> {
    match provider {
        Provider::Claude => vec![
            vector.fresh_input_tokens,
            vector.cache_read_input_tokens,
            vector.cache_write_5m_input_tokens,
            vector.cache_write_1h_input_tokens,
            vector.output_tokens,
        ],
        Provider::Codex => vec![
            vector.fresh_input_tokens,
            vector.cache_read_input_tokens,
            vector.output_tokens,
        ],
    }
}

fn classify_changes(reduced: bool, increased: bool, reasons: &mut Vec<&'static str>) -> Outcome {
    match (reduced, increased) {
        (true, false) => Outcome::SupportedCandidate,
        (false, true) => {
            reasons.push("token-proxy-regression");
            Outcome::Rejected
        }
        (true, true) => {
            reasons.push("token-dimension-tradeoff");
            Outcome::Inconclusive
        }
        (false, false) => {
            reasons.push("no-token-proxy-reduction");
            Outcome::Inconclusive
        }
    }
}
