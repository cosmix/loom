//! Relates session length to context carried into requests, so orchestration
//! policy can distinguish useful long-lived work from accumulating context.

use crate::commands::usage::accounting::Accounting;
use crate::commands::usage::transcript::Transcript;

use super::fmt::{format_f64, format_u64, heading, no_data};

#[derive(Debug, serde::Serialize)]
pub struct SessionLengths {
    pub buckets: Vec<LengthBucket>,
    pub resident_context: Vec<ResidentPoint>,
    pub peak_resident: PeakResident,
}

#[derive(Debug, serde::Serialize)]
pub struct LengthBucket {
    pub label: String,
    pub transcripts: usize,
    pub s1_share: f64,
    pub s2_share: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct ResidentPoint {
    pub request_index: usize,
    pub samples: usize,
    pub median: u64,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct PeakResident {
    pub samples: usize,
    pub p50: u64,
    pub p90: u64,
    pub p99: u64,
    pub max: u64,
}

pub fn build(transcripts: &[Transcript]) -> SessionLengths {
    let accounting = total_accounting(transcripts);
    let buckets = length_buckets(transcripts, accounting);
    let resident_context = resident_points(transcripts);
    let peak_resident = peak_stats(transcripts);
    SessionLengths {
        buckets,
        resident_context,
        peak_resident,
    }
}

pub fn render(report: &SessionLengths) {
    heading("Session lengths");
    if report.buckets.iter().all(|bucket| bucket.transcripts == 0) {
        no_data("session lengths");
        return;
    }
    println!("  request-count buckets:");
    for bucket in &report.buckets {
        println!(
            "    {}: {} transcripts, S1 {}%, S2 {}%",
            bucket.label,
            bucket.transcripts,
            format_f64(bucket.s1_share),
            format_f64(bucket.s2_share)
        );
    }
    println!("  median resident context:");
    for point in report
        .resident_context
        .iter()
        .filter(|point| point.samples > 0)
    {
        println!(
            "    request {}: {} tokens ({} sessions)",
            point.request_index,
            format_u64(point.median),
            point.samples
        );
    }
    let peak = &report.peak_resident;
    println!(
        "  peak resident context ({} sessions): p50 {} p90 {} p99 {} max {}",
        peak.samples,
        format_u64(peak.p50),
        format_u64(peak.p90),
        format_u64(peak.p99),
        format_u64(peak.max)
    );
}

fn total_accounting(transcripts: &[Transcript]) -> Accounting {
    let mut total = Accounting::default();
    for transcript in transcripts {
        total.add(&Accounting::of(&transcript.total_usage()));
    }
    total
}

fn length_buckets(transcripts: &[Transcript], total: Accounting) -> Vec<LengthBucket> {
    let ranges = [
        ("1-10", 1, 10),
        ("11-25", 11, 25),
        ("26-50", 26, 50),
        ("51-100", 51, 100),
        ("101-200", 101, 200),
        ("201-400", 201, 400),
        ("400+", 401, usize::MAX),
    ];
    ranges
        .into_iter()
        .map(|(label, low, high)| bucket_for(transcripts, total, label, low, high))
        .collect()
}

fn bucket_for(
    transcripts: &[Transcript],
    total: Accounting,
    label: &str,
    low: usize,
    high: usize,
) -> LengthBucket {
    let selected = transcripts
        .iter()
        .filter(|item| {
            let count = item.requests().count();
            count >= low && count <= high
        })
        .collect::<Vec<_>>();
    let mut usage = Accounting::default();
    for transcript in &selected {
        usage.add(&Accounting::of(&transcript.total_usage()));
    }
    LengthBucket {
        label: label.to_owned(),
        transcripts: selected.len(),
        s1_share: share(usage.s1 as f64, total.s1 as f64),
        s2_share: share(usage.s2, total.s2),
    }
}

fn resident_points(transcripts: &[Transcript]) -> Vec<ResidentPoint> {
    [1, 10, 30, 50, 100]
        .into_iter()
        .map(|index| {
            let mut values = transcripts
                .iter()
                .filter_map(|transcript| {
                    transcript
                        .requests()
                        .nth(index - 1)
                        .map(|request| request.usage.resident())
                })
                .collect::<Vec<_>>();
            let samples = values.len();
            ResidentPoint {
                request_index: index,
                samples,
                median: percentile(&mut values, 0.5),
            }
        })
        .collect()
}

fn peak_stats(transcripts: &[Transcript]) -> PeakResident {
    let mut peaks = transcripts
        .iter()
        .filter_map(|transcript| {
            transcript
                .requests()
                .map(|request| request.usage.resident())
                .max()
        })
        .collect::<Vec<_>>();
    let samples = peaks.len();
    let p50 = percentile(&mut peaks.clone(), 0.5);
    let p90 = percentile(&mut peaks.clone(), 0.9);
    let p99 = percentile(&mut peaks, 0.99);
    let max = peaks.iter().max().copied().unwrap_or(0);
    PeakResident {
        samples,
        p50,
        p90,
        p99,
        max,
    }
}

fn percentile(values: &mut [u64], fraction: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = ((values.len() as f64 * fraction).ceil() as usize).saturating_sub(1);
    values[index]
}

fn share(value: f64, total: f64) -> f64 {
    if total == 0.0 {
        0.0
    } else {
        value * 100.0 / total
    }
}
