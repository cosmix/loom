//! Peak resident context by scope, so orchestration policy can see how much
//! headroom a session actually used rather than only its running totals.
//! A subagent's peak is also compared against its own boot cost: a peak
//! that never exceeds twice the first request's resident size never grew
//! past whatever context its spawn prompt itself carried in.

use crate::commands::usage::transcript::{Scope, Transcript};

use super::fmt::{format_f64, format_u64, heading, no_data, percentile, share};

const ABOVE_250K: u64 = 250_000;
const ABOVE_400K: u64 = 400_000;

/// Splits the peak resident context `lengths` reports corpus-wide out by
/// scope (main vs. subagent).
#[derive(Debug, serde::Serialize)]
pub struct Peaks {
    pub main: ScopePeaks,
    pub subagent: ScopePeaks,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct ScopePeaks {
    pub count: usize,
    pub p50: u64,
    pub p90: u64,
    pub max: u64,
    pub share_above_250k: f64,
    pub share_above_400k: f64,
    /// Subagents only: share of transcripts whose peak never exceeded twice
    /// their first request's resident size. `None` for the main scope.
    pub boot_dominated_share: Option<f64>,
}

pub fn build(transcripts: &[Transcript]) -> Peaks {
    Peaks {
        main: scope_peaks(transcripts, Scope::Main),
        subagent: scope_peaks(transcripts, Scope::Subagent),
    }
}

pub fn render(report: &Peaks) {
    heading("Peak resident context by scope");
    render_scope("main", &report.main);
    render_scope("subagent", &report.subagent);
}

fn render_scope(label: &str, peaks: &ScopePeaks) {
    if peaks.count == 0 {
        no_data(label);
        return;
    }
    println!(
        "  {label} ({} sessions): p50 {} p90 {} max {}, share >250k {}%, share >400k {}%",
        peaks.count,
        format_u64(peaks.p50),
        format_u64(peaks.p90),
        format_u64(peaks.max),
        format_f64(peaks.share_above_250k),
        format_f64(peaks.share_above_400k)
    );
    if let Some(boot_dominated) = peaks.boot_dominated_share {
        println!(
            "    boot-dominated (peak < 2x first request): {}%",
            format_f64(boot_dominated)
        );
    }
}

fn scope_peaks(transcripts: &[Transcript], scope: Scope) -> ScopePeaks {
    let selected = transcripts
        .iter()
        .filter(|transcript| transcript.scope == scope && transcript.requests().next().is_some())
        .collect::<Vec<_>>();
    let mut values = selected
        .iter()
        .map(|item| peak_resident(item))
        .collect::<Vec<_>>();
    let count = values.len();
    let p50 = percentile(&mut values.clone(), 0.5);
    let p90 = percentile(&mut values, 0.9);
    let max = values.iter().max().copied().unwrap_or(0);
    let boot_dominated_share = (scope == Scope::Subagent).then(|| boot_dominated_share(&selected));
    ScopePeaks {
        count,
        p50,
        p90,
        max,
        share_above_250k: share(count_above(&selected, ABOVE_250K) as f64, count as f64),
        share_above_400k: share(count_above(&selected, ABOVE_400K) as f64, count as f64),
        boot_dominated_share,
    }
}

fn peak_resident(transcript: &Transcript) -> u64 {
    transcript
        .requests()
        .map(|request| request.usage.resident())
        .max()
        .unwrap_or(0)
}

fn count_above(transcripts: &[&Transcript], threshold: u64) -> usize {
    transcripts
        .iter()
        .filter(|transcript| peak_resident(transcript) > threshold)
        .count()
}

fn boot_dominated_share(subagents: &[&Transcript]) -> f64 {
    let boot_dominated = subagents
        .iter()
        .filter(|transcript| {
            let boot = transcript
                .requests()
                .next()
                .map_or(0, |request| request.usage.resident());
            peak_resident(transcript) < boot.saturating_mul(2)
        })
        .count();
    share(boot_dominated as f64, subagents.len() as f64)
}

#[cfg(test)]
#[path = "peaks_tests.rs"]
mod tests;
