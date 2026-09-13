use std::collections::BTreeMap;

use super::comparison_schema::{Outcome, QuotaComparisonEvidence, QuotaObservation, RunEvidence};
use super::provider_types::Provider;
use crate::quota::{HistoryContinuity, WindowKind};

type QuotaKey = (Provider, u8);

pub(super) fn evaluate(
    baseline: &RunEvidence,
    candidate: &RunEvidence,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    let Some(baseline) = observation_map(baseline) else {
        reasons.push("missing-quota-telemetry");
        return Outcome::Inconclusive;
    };
    let Some(candidate) = observation_map(candidate) else {
        reasons.push("missing-quota-telemetry");
        return Outcome::Inconclusive;
    };
    if baseline.keys().ne(candidate.keys()) {
        reasons.push("missing-quota-window");
        return Outcome::Inconclusive;
    }
    compare_observations(&baseline, &candidate, reasons)
}

pub(super) fn evidence(
    baseline: &RunEvidence,
    candidate: &RunEvidence,
) -> Vec<QuotaComparisonEvidence> {
    let (Some(baseline), Some(candidate)) = (observation_map(baseline), observation_map(candidate))
    else {
        return Vec::new();
    };
    baseline
        .iter()
        .filter_map(|(key, baseline)| {
            let candidate = candidate.get(key)?;
            Some(QuotaComparisonEvidence {
                provider: baseline.provider,
                window: baseline.window,
                baseline_consumption_percent: consumption_delta(baseline),
                candidate_consumption_percent: consumption_delta(candidate),
                combined_uncertainty_percent: uncertainty(baseline, candidate),
            })
        })
        .collect()
}

fn observation_map(run: &RunEvidence) -> Option<BTreeMap<QuotaKey, &QuotaObservation>> {
    let observations = run.quota_observations.as_deref()?;
    if observations.is_empty() {
        return None;
    }
    let mut mapped = BTreeMap::new();
    for observation in observations {
        if mapped
            .insert(observation_key(observation), observation)
            .is_some()
        {
            return None;
        }
    }
    Some(mapped)
}

fn compare_observations(
    baseline: &BTreeMap<QuotaKey, &QuotaObservation>,
    candidate: &BTreeMap<QuotaKey, &QuotaObservation>,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    let mut verdict = Outcome::SupportedCandidate;
    let mut improved = false;
    for (key, baseline) in baseline {
        let candidate = candidate[key];
        let interval = compare_interval(baseline, candidate, reasons);
        improved |= interval == Outcome::SupportedCandidate;
        verdict = verdict.worse(interval);
    }
    if verdict == Outcome::Rejected {
        return verdict;
    }
    if !improved && verdict == Outcome::SupportedCandidate {
        reasons.push("quota-reduction-not-established");
        return Outcome::Inconclusive;
    }
    verdict
}

fn compare_interval(
    baseline: &QuotaObservation,
    candidate: &QuotaObservation,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    if interval_duration(baseline) != interval_duration(candidate) {
        reasons.push("quota-interval-mismatch");
        return Outcome::Inconclusive;
    }
    if reset_incomparable(baseline, candidate) {
        reasons.push("quota-reset-crossing");
        return Outcome::Inconclusive;
    }
    if unrelated_use(baseline) || unrelated_use(candidate) {
        reasons.push("unrelated-concurrent-quota-use");
        return Outcome::Inconclusive;
    }
    let Some(uncertainty) = uncertainty(baseline, candidate) else {
        reasons.push("unknown-quota-precision");
        return Outcome::Inconclusive;
    };
    let Some(baseline_delta) = consumption_delta(baseline) else {
        reasons.push("same-reset-utilization-decrease");
        return Outcome::Inconclusive;
    };
    let Some(candidate_delta) = consumption_delta(candidate) else {
        reasons.push("same-reset-utilization-decrease");
        return Outcome::Inconclusive;
    };
    classify_delta(baseline_delta, candidate_delta, uncertainty, reasons)
}

fn interval_duration(observation: &QuotaObservation) -> Option<i64> {
    observation
        .observed_end_at
        .checked_sub(observation.observed_start_at)
}

fn reset_incomparable(baseline: &QuotaObservation, candidate: &QuotaObservation) -> bool {
    baseline.reset_id != candidate.reset_id
        || baseline.resets_at != candidate.resets_at
        || baseline.continuity != HistoryContinuity::SameReset
        || candidate.continuity != HistoryContinuity::SameReset
        || baseline.observed_end_at >= baseline.resets_at
        || candidate.observed_end_at >= candidate.resets_at
}

fn unrelated_use(observation: &QuotaObservation) -> bool {
    observation.unrelated_concurrent_use != Some(false)
}

fn observation_key(observation: &QuotaObservation) -> QuotaKey {
    let window = match observation.window {
        WindowKind::FiveHour => 0,
        WindowKind::SevenDay => 1,
    };
    (observation.provider, window)
}

fn uncertainty(baseline: &QuotaObservation, candidate: &QuotaObservation) -> Option<f64> {
    Some(baseline.precision_percent? + candidate.precision_percent?)
}

fn consumption_delta(observation: &QuotaObservation) -> Option<f64> {
    let delta = observation.used_percent_end - observation.used_percent_start;
    (delta >= 0.0).then_some(delta)
}

fn classify_delta(
    baseline: f64,
    candidate: f64,
    uncertainty: f64,
    reasons: &mut Vec<&'static str>,
) -> Outcome {
    if candidate - baseline > uncertainty {
        reasons.push("subscription-consumption-regression");
        Outcome::Rejected
    } else if baseline - candidate > uncertainty {
        Outcome::SupportedCandidate
    } else {
        reasons.push("quota-change-within-uncertainty");
        Outcome::Inconclusive
    }
}
