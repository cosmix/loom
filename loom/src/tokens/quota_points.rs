//! The flow's `quota_left` entries: each provider's recent five-hour usage
//! points, split into reset periods, fed to [`estimate`].

use super::model::{QuotaLeft, QuotaProvider};
use super::quota_left::{estimate, SLOPE_WINDOW_SECS};
use crate::quota::cache::{provider_path, quota_dir};
use crate::quota::{read_history, read_snapshot, ProviderQuota, QuotaWindow, WindowKind};
use std::path::{Path, PathBuf};

/// Five-hour `resets_at` values further apart than this belong to different
/// periods. Real history alternates `resets_at` by 1 s between polls.
const RESET_SLACK_SECS: u64 = 300;

/// One five-hour reading.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FivePoint {
    at: i64,
    used: f64,
    resets_at: Option<i64>,
}

/// Claude's then Codex's time left, for each provider whose newest point has
/// a five-hour window.
pub(super) fn quota_left(work_root: &Path, now: i64) -> Vec<QuotaLeft> {
    let snapshot = read_snapshot(work_root);
    [
        (QuotaProvider::Claude, "claude", snapshot.claude),
        (QuotaProvider::Codex, "codex", snapshot.codex),
    ]
    .into_iter()
    .filter_map(|(provider, name, latest)| {
        let points = five_hour_points(work_root, name, latest.as_ref(), now)?;
        let (period, resets_at) = newest_period(&points);
        let left = estimate(&period, resets_at, now);
        Some(QuotaLeft {
            provider,
            minutes_left: left.minutes_left,
            lasts_to_reset: left.lasts_to_reset,
        })
    })
    .collect()
}

/// Every file [`quota_left`] reads: each provider's cached snapshot, then
/// each provider's history (`quota/history/<provider>.jsonl`, where
/// `read_history` reads it).
pub(super) fn quota_inputs(work_root: &Path) -> [PathBuf; 4] {
    let history = quota_dir(work_root).join("history");
    [
        provider_path(work_root, "claude"),
        provider_path(work_root, "codex"),
        history.join("claude.jsonl"),
        history.join("codex.jsonl"),
    ]
}

/// The provider's five-hour points of the last [`SLOPE_WINDOW_SECS`], oldest
/// first, with the cached snapshot appended when it carries a five-hour window
/// observed after the newest history row (the history store skips a row equal
/// to the last one). `None` when the newest point has no five-hour window.
/// History continuity marks are ignored: they flag the 1 s `resets_at` jitter
/// and seven-day window changes as resets.
fn five_hour_points(
    work_root: &Path,
    provider: &str,
    latest: Option<&ProviderQuota>,
    now: i64,
) -> Option<Vec<FivePoint>> {
    let history = read_history(work_root, provider, now - SLOPE_WINDOW_SECS, Some(now));
    let mut points: Vec<(i64, Option<&QuotaWindow>)> = history
        .points
        .iter()
        .map(|point| (point.observed_at, five_hour(&point.windows)))
        .collect();
    let newest_row = points.last().map(|(at, _)| *at);
    if let Some(latest) = latest.filter(|q| newest_row.is_none_or(|at| q.observed_at > at)) {
        if let Some(window) = five_hour(&latest.windows) {
            points.push((latest.observed_at, Some(window)));
        }
    }
    if !points.last().is_some_and(|(_, window)| window.is_some()) {
        return None;
    }
    let readings = points.into_iter().filter_map(|(at, window)| {
        window.map(|window| FivePoint {
            at,
            used: window.used_percent,
            resets_at: window.resets_at,
        })
    });
    Some(readings.collect())
}

fn five_hour(windows: &[QuotaWindow]) -> Option<&QuotaWindow> {
    windows
        .iter()
        .find(|window| window.kind == WindowKind::FiveHour)
}

/// The points of the newest reset period as `(observed_at, used_percent)`,
/// and the newest point's `resets_at`.
fn newest_period(points: &[FivePoint]) -> (Vec<(i64, f64)>, Option<i64>) {
    let start = points
        .windows(2)
        .rposition(|pair| starts_period(&pair[0], &pair[1]))
        .map_or(0, |index| index + 1);
    let period = &points[start..];
    let readings = period.iter().map(|point| (point.at, point.used)).collect();
    (readings, period.last().and_then(|point| point.resets_at))
}

/// Whether `next` opens a new period after `previous`: its usage dropped, or
/// its `resets_at` moved more than [`RESET_SLACK_SECS`] (only when both are
/// known).
fn starts_period(previous: &FivePoint, next: &FivePoint) -> bool {
    let moved = match (previous.resets_at, next.resets_at) {
        (Some(before), Some(after)) => before.abs_diff(after) > RESET_SLACK_SECS,
        _ => false,
    };
    next.used < previous.used || moved
}

#[cfg(test)]
#[path = "quota_points_tests.rs"]
mod tests;
