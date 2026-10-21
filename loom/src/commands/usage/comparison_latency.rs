use serde::Serialize;

use super::comparison_schema::{ComparisonPair, Outcome};

#[derive(Debug, Serialize)]
pub(super) struct LatencySummary {
    pub(super) verdict: Outcome,
    pub(super) tolerance_ms: u64,
    pub(super) critical_path: MeasureSummary,
    pub(super) notification: MeasureSummary,
}

#[derive(Debug, Serialize)]
pub(super) struct MeasureSummary {
    pub(super) state: &'static str,
    pub(super) pairs: usize,
    pub(super) mean_increase_ms: Option<f64>,
    pub(super) verdict: Outcome,
}

enum Measure {
    CriticalPath,
    Notification,
}

enum Presence {
    AllPresent(i128),
    AllAbsent,
    Partial,
}

pub(super) fn evaluate(
    pairs: &[&ComparisonPair],
    tolerance_ms: u64,
    reasons: &mut Vec<&'static str>,
) -> LatencySummary {
    let critical_path = evaluate_measure(
        pairs,
        tolerance_ms,
        Measure::CriticalPath,
        |pair| {
            (
                pair.baseline.critical_path_ms,
                pair.candidate.critical_path_ms,
            )
        },
        reasons,
    );
    let notification = evaluate_measure(
        pairs,
        tolerance_ms,
        Measure::Notification,
        |pair| {
            (
                pair.baseline.notification_latency_ms,
                pair.candidate.notification_latency_ms,
            )
        },
        reasons,
    );
    let verdict = critical_path.verdict.worse(notification.verdict);

    LatencySummary {
        verdict,
        tolerance_ms,
        critical_path,
        notification,
    }
}

fn evaluate_measure<F>(
    pairs: &[&ComparisonPair],
    tolerance_ms: u64,
    measure: Measure,
    values: F,
    reasons: &mut Vec<&'static str>,
) -> MeasureSummary
where
    F: Fn(&ComparisonPair) -> (Option<u64>, Option<u64>),
{
    if pairs.is_empty() {
        return unmeasured_measure(measure, true, reasons);
    }

    match presence(pairs, values) {
        Presence::AllPresent(total_increase) => {
            let mean_increase_ms = total_increase as f64 / pairs.len() as f64;
            let verdict = if mean_increase_ms > tolerance_ms as f64 {
                reasons.push(regression_reason(measure));
                Outcome::Rejected
            } else {
                Outcome::SupportedCandidate
            };
            MeasureSummary {
                state: "judged",
                pairs: pairs.len(),
                mean_increase_ms: Some(mean_increase_ms),
                verdict,
            }
        }
        Presence::AllAbsent => unmeasured_measure(measure, false, reasons),
        Presence::Partial => {
            reasons.push("missing-latency-evidence");
            MeasureSummary {
                state: "partial",
                pairs: 0,
                mean_increase_ms: None,
                verdict: Outcome::Inconclusive,
            }
        }
    }
}

fn presence<F>(pairs: &[&ComparisonPair], values: F) -> Presence
where
    F: Fn(&ComparisonPair) -> (Option<u64>, Option<u64>),
{
    let mut total_increase = 0_i128;
    let mut all_present = true;
    let mut all_absent = true;
    for pair in pairs {
        match values(pair) {
            (Some(baseline), Some(candidate)) => {
                total_increase += i128::from(candidate) - i128::from(baseline);
                all_absent = false;
            }
            (None, None) => all_present = false,
            _ => {
                all_present = false;
                all_absent = false;
            }
        }
    }

    if all_present {
        Presence::AllPresent(total_increase)
    } else if all_absent {
        Presence::AllAbsent
    } else {
        Presence::Partial
    }
}

fn regression_reason(measure: Measure) -> &'static str {
    match measure {
        Measure::CriticalPath => "critical-path-regression",
        Measure::Notification => "notification-latency-regression",
    }
}

fn unmeasured_measure(
    measure: Measure,
    no_pairs: bool,
    reasons: &mut Vec<&'static str>,
) -> MeasureSummary {
    let verdict = match measure {
        Measure::CriticalPath => {
            reasons.push("missing-latency-evidence");
            Outcome::Inconclusive
        }
        Measure::Notification => {
            if !no_pairs {
                reasons.push("notification-latency-unmeasured");
            }
            Outcome::SupportedCandidate
        }
    };
    MeasureSummary {
        state: "unmeasured",
        pairs: 0,
        mean_increase_ms: None,
        verdict,
    }
}

#[cfg(test)]
#[path = "comparison_latency_tests.rs"]
mod tests;
