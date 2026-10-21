use std::collections::BTreeSet;

use serde::Serialize;

use super::comparison_schema::{ComparisonPair, Outcome};
use super::provider_types::Provider;

#[derive(Debug, Serialize)]
pub(super) struct NetTotalSummary {
    pub(super) verdict: Outcome,
    pub(super) reason_codes: Vec<&'static str>,
    pub(super) providers: Vec<ProviderNetTotal>,
}

#[derive(Debug, Serialize)]
pub(super) struct ProviderNetTotal {
    pub(super) provider: Provider,
    pub(super) pairs: usize,
    pub(super) baseline_total: u64,
    pub(super) candidate_total: u64,
    pub(super) mean_reduction: Option<f64>,
    pub(super) standard_error: Option<f64>,
    pub(super) classification: &'static str,
}

pub(super) fn evaluate(pairs: &[&ComparisonPair], excluded_pairs: usize) -> NetTotalSummary {
    if excluded_pairs > 0 {
        return summary(
            Outcome::Inconclusive,
            "net-total-excluded-pairs",
            Vec::new(),
        );
    }
    if pairs.is_empty() {
        return summary(Outcome::Inconclusive, "insufficient-pairs", Vec::new());
    }
    if cross_provider_transfer(pairs) {
        return summary(Outcome::Inconclusive, "cross-provider-transfer", Vec::new());
    }

    let providers = providers(pairs)
        .into_iter()
        .map(|provider| provider_summary(provider, pairs))
        .collect::<Vec<_>>();
    let (verdict, reason) = summary_result(&providers);
    summary(verdict, reason, providers)
}

fn summary(
    verdict: Outcome,
    reason: &'static str,
    providers: Vec<ProviderNetTotal>,
) -> NetTotalSummary {
    NetTotalSummary {
        verdict,
        reason_codes: if reason.is_empty() {
            Vec::new()
        } else {
            vec![reason]
        },
        providers,
    }
}

fn cross_provider_transfer(pairs: &[&ComparisonPair]) -> bool {
    pairs.iter().any(|pair| {
        pair.baseline
            .provider_tokens
            .keys()
            .ne(pair.candidate.provider_tokens.keys())
    })
}

fn providers(pairs: &[&ComparisonPair]) -> BTreeSet<Provider> {
    pairs
        .iter()
        .flat_map(|pair| pair.baseline.provider_tokens.keys().copied())
        .collect()
}

fn provider_summary(provider: Provider, pairs: &[&ComparisonPair]) -> ProviderNetTotal {
    let mut pair_count = 0;
    let mut baseline_total: u64 = 0;
    let mut candidate_total: u64 = 0;
    let mut deltas = Vec::new();
    let mut missing_dimension = false;

    for pair in pairs {
        let Some(baseline) = pair.baseline.provider_tokens.get(&provider) else {
            continue;
        };
        let Some(candidate) = pair.candidate.provider_tokens.get(&provider) else {
            continue;
        };
        pair_count += 1;
        let baseline_net = net(baseline);
        let candidate_net = net(candidate);
        if let Some(total) = baseline_net {
            baseline_total = baseline_total.saturating_add(total);
        }
        if let Some(total) = candidate_net {
            candidate_total = candidate_total.saturating_add(total);
        }
        let Some((baseline_net, candidate_net)) = baseline_net.zip(candidate_net) else {
            missing_dimension = true;
            continue;
        };
        deltas.push(i128::from(baseline_net) - i128::from(candidate_net));
    }

    let (classification, mean_reduction, standard_error) = if missing_dimension {
        ("missing-dimension", None, None)
    } else if pair_count < 3 {
        ("insufficient-pairs", None, None)
    } else {
        let (mean, error) = mean_and_standard_error(&deltas);
        (classification(mean, error), Some(mean), Some(error))
    };
    ProviderNetTotal {
        provider,
        pairs: pair_count,
        baseline_total,
        candidate_total,
        mean_reduction,
        standard_error,
        classification,
    }
}

fn net(vector: &super::provider_types::ProviderTokenVector) -> Option<u64> {
    vector
        .resident_input_tokens?
        .checked_add(vector.output_tokens?)
}

fn mean_and_standard_error(deltas: &[i128]) -> (f64, f64) {
    let count = deltas.len() as f64;
    let mean = deltas.iter().map(|delta| *delta as f64).sum::<f64>() / count;
    let variance = deltas
        .iter()
        .map(|delta| (*delta as f64 - mean).powi(2))
        .sum::<f64>()
        / (count - 1.0);
    (mean, (variance / count).sqrt())
}

fn classification(mean: f64, standard_error: f64) -> &'static str {
    if mean > 0.0 && mean > 2.0 * standard_error {
        "reduction"
    } else if mean < 0.0 && -mean > 2.0 * standard_error {
        "regression"
    } else {
        "within-noise"
    }
}

fn summary_result(providers: &[ProviderNetTotal]) -> (Outcome, &'static str) {
    if providers
        .iter()
        .any(|provider| provider.classification == "missing-dimension")
    {
        return (Outcome::Inconclusive, "missing-token-dimension");
    }
    if providers
        .iter()
        .any(|provider| provider.classification == "insufficient-pairs")
    {
        return (Outcome::Inconclusive, "insufficient-pairs");
    }

    let reduction = providers
        .iter()
        .any(|provider| provider.classification == "reduction");
    let regression = providers
        .iter()
        .any(|provider| provider.classification == "regression");
    if !providers.is_empty()
        && providers
            .iter()
            .all(|provider| provider.classification == "reduction")
    {
        (Outcome::SupportedCandidate, "")
    } else if reduction && regression {
        (Outcome::Inconclusive, "cross-provider-transfer")
    } else if regression {
        (Outcome::Rejected, "net-total-regression")
    } else {
        (Outcome::Inconclusive, "net-total-within-noise")
    }
}

#[cfg(test)]
#[path = "comparison_net_tests.rs"]
mod tests;
