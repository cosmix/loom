//! Groups requests into decision-sized time slices so an operator can see
//! whether a run's cost was concentrated in a burst or spread over time.
//! A five-hour "window" is deliberately approximate: it is a cluster split
//! by five or more hours of silence, not a billing-period reconstruction.

use crate::commands::usage::accounting::{bucket, Accounting, Window, Windowing};
use crate::commands::usage::transcript::{TokenUsage, Transcript};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;

use super::fmt::{format_f64, format_u64, heading, no_data};

#[derive(Debug, serde::Serialize)]
pub struct WindowReport {
    pub windows: Vec<WindowRow>,
    pub totals: WindowRow,
}

#[derive(Debug, serde::Serialize)]
pub struct WindowRow {
    pub label: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub requests: usize,
    pub fresh_input: u64,
    pub cache_creation: u64,
    pub cache_reads: u64,
    pub output: u64,
    pub cache_writes_5m: u64,
    pub cache_writes_1h: u64,
    pub s1: u64,
    pub s2: f64,
    pub s3: u64,
}

pub fn build(transcripts: &[Transcript], windowing: Windowing) -> WindowReport {
    let mut requests = transcripts
        .iter()
        .flat_map(Transcript::requests)
        .filter(|request| request.model != SYNTHETIC_MODEL)
        .collect::<Vec<_>>();
    requests.sort_by_key(|request| request.timestamp);
    let windows = bucket(&requests, windowing);
    let mut usage = TokenUsage::default();
    for request in &requests {
        usage.add(&request.usage);
    }
    WindowReport {
        windows: windows.iter().map(window_row).collect(),
        totals: window_row(&Window {
            label: "all windows".to_owned(),
            start: requests
                .first()
                .map(|request| request.timestamp)
                .unwrap_or_else(chrono::Utc::now),
            end: requests
                .last()
                .map(|request| request.timestamp)
                .unwrap_or_else(chrono::Utc::now),
            requests: requests.len(),
            usage,
            accounting: Accounting::of(&usage),
        }),
    }
}

pub fn render(report: &WindowReport) {
    heading("Windows (tokens)");
    if report.windows.is_empty() {
        no_data("windows");
        return;
    }
    for window in &report.windows {
        render_window(window);
    }
    println!("  totals:");
    render_values(&report.totals);
}

fn render_window(window: &WindowRow) {
    println!("  {}:", window.label);
    render_values(window);
}

fn render_values(window: &WindowRow) {
    detail("requests", window.requests);
    detail("fresh input", format_u64(window.fresh_input));
    detail("cache creation", format_u64(window.cache_creation));
    detail("cache writes (5m)", format_u64(window.cache_writes_5m));
    detail("cache writes (1h)", format_u64(window.cache_writes_1h));
    detail("cache reads", format_u64(window.cache_reads));
    detail("output", format_u64(window.output));
    detail("S1", format_u64(window.s1));
    detail("S2", format_f64(window.s2));
    detail("S3", format_u64(window.s3));
}

/// Prints one level deeper than a window's own `{label}:` heading, so the
/// window/detail hierarchy is visible in the indentation.
fn detail(label: &str, value: impl std::fmt::Display) {
    println!("    {label}: {value}");
}

fn window_row(window: &Window) -> WindowRow {
    WindowRow {
        label: window.label.clone(),
        start: window.start,
        end: window.end,
        requests: window.requests,
        fresh_input: window.usage.input,
        cache_creation: window.usage.cache_creation,
        cache_reads: window.usage.cache_read,
        output: window.usage.output,
        cache_writes_5m: window.usage.ephemeral_5m,
        cache_writes_1h: window.usage.ephemeral_1h,
        s1: window.accounting.s1,
        s2: window.accounting.s2,
        s3: window.accounting.s3,
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
