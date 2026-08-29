//! Keeps the report's three token measures explicit. They are intentionally
//! accounting views, not prices: rates change while the transcript's token
//! record remains useful, and a read-only command should not suggest a
//! monetary precision it cannot guarantee.

use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Accounting {
    /// Every token counted once: resident input plus output.
    pub s1: u64,
    /// API-shaped weighting: fresh input x1, cache reads x0.1,
    /// 5m cache writes x1.25, 1h cache writes x2, output x5.
    pub s2: f64,
    /// Cache writes plus fresh input plus output; cache reads excluded.
    pub s3: u64,
}

impl Accounting {
    pub fn of(usage: &super::transcript::TokenUsage) -> Accounting {
        Accounting {
            s1: usage.resident() + usage.output,
            s2: usage.input as f64
                + usage.cache_read as f64 * 0.1
                + usage.ephemeral_5m as f64 * 1.25
                + usage.ephemeral_1h as f64 * 2.0
                + usage.output as f64 * 5.0,
            s3: usage.cache_creation + usage.input + usage.output,
        }
    }
    pub fn add(&mut self, other: &Accounting) {
        self.s1 += other.s1;
        self.s2 += other.s2;
        self.s3 += other.s3;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Windowing {
    #[value(name = "5h")]
    FiveHour,
    #[value(name = "week")]
    IsoWeek,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Window {
    /// Human label: an RFC3339-ish start stamp for `FiveHour`, `2026-W35` for `IsoWeek`.
    pub label: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub requests: usize,
    pub usage: super::transcript::TokenUsage,
    pub accounting: Accounting,
}

/// Bucket requests into windows, oldest first. `FiveHour` starts a new window
/// whenever 5+ hours of silence separate consecutive requests - an
/// approximation of the API's rolling limit window, deliberately not a
/// calendar-aligned scheduler. `IsoWeek` groups by ISO year and week number.
pub fn bucket(requests: &[&super::transcript::Request], mode: Windowing) -> Vec<Window> {
    let mut ordered = requests.to_vec();
    ordered.sort_by_key(|request| request.timestamp);
    match mode {
        Windowing::FiveHour => five_hour_windows(&ordered),
        Windowing::IsoWeek => iso_week_windows(&ordered),
    }
}

/// The silence that ends a 5-hour window.
pub const IDLE_GAP_HOURS: i64 = 5;

fn five_hour_windows(requests: &[&super::transcript::Request]) -> Vec<Window> {
    let mut windows = Vec::new();
    let Some(first) = requests.first() else {
        return windows;
    };
    let mut window = request_window(first, first.timestamp.to_rfc3339());
    let mut previous = first.timestamp;
    for request in requests.iter().skip(1) {
        if request.timestamp - previous >= Duration::hours(IDLE_GAP_HOURS) {
            windows.push(window);
            window = request_window(request, request.timestamp.to_rfc3339());
        } else {
            add_request(&mut window, request);
        }
        previous = request.timestamp;
    }
    windows.push(window);
    windows
}

fn iso_week_windows(requests: &[&super::transcript::Request]) -> Vec<Window> {
    let mut windows: Vec<Window> = Vec::new();
    for request in requests {
        let week = request.timestamp.iso_week();
        let label = format!("{}-W{:02}", week.year(), week.week());
        if let Some(window) = windows.last_mut() {
            if window.label == label {
                add_request(window, request);
                continue;
            }
        }
        // `week_window` always returns a window (falling back to the
        // request's own timestamp if the calendar date fails to resolve),
        // never `None` - dropping a request here would silently disagree
        // with the report's own Totals section.
        windows.push(week_window(request, label, week.year(), week.week()));
    }
    windows
}

fn request_window(request: &super::transcript::Request, label: String) -> Window {
    Window {
        label,
        start: request.timestamp,
        end: request.timestamp,
        requests: 1,
        usage: request.usage,
        accounting: Accounting::of(&request.usage),
    }
}

fn week_window(
    request: &super::transcript::Request,
    label: String,
    year: i32,
    week: u32,
) -> Window {
    let start = NaiveDate::from_isoywd_opt(year, week, Weekday::Mon)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| Utc.from_utc_datetime(&naive))
        // Falls back to the request's own timestamp so a week that fails to
        // resolve to a calendar date still keeps the request in the report.
        .unwrap_or(request.timestamp);
    Window {
        end: start + Duration::weeks(1),
        start,
        label,
        requests: 1,
        usage: request.usage,
        accounting: Accounting::of(&request.usage),
    }
}

fn add_request(window: &mut Window, request: &super::transcript::Request) {
    window.requests += 1;
    window.usage.add(&request.usage);
    window.accounting.add(&Accounting::of(&request.usage));
    if request.timestamp > window.end {
        window.end = request.timestamp;
    }
}

#[cfg(test)]
#[path = "accounting_tests.rs"]
mod tests;
