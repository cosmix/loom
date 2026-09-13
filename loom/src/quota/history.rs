//! Bounded, reset-aware quota observations for later reporting consumers.

use super::cache;
use super::model::{ProviderQuota, QuotaWindow, WindowKind};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[path = "history_store.rs"]
mod history_store;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaHistoryPoint {
    pub observed_at: i64,
    pub windows: Vec<QuotaWindow>,
    pub plan: Option<String>,
    pub continuity: HistoryContinuity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryContinuity {
    Initial,
    SameReset,
    Reset,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuotaHistoryRead {
    pub points: Vec<QuotaHistoryPoint>,
    pub diagnostics: QuotaHistoryDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuotaHistoryDiagnostics {
    pub source: HistorySourceState,
    pub malformed_rows: usize,
    pub unsupported_schema_rows: usize,
    pub nonmonotonic_rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HistorySourceState {
    Missing,
    Empty,
    Read,
    Corrupt,
    Unreadable,
    UnknownProvider,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPoint {
    schema_version: u8,
    observed_at: i64,
    windows: Vec<QuotaWindow>,
    plan: Option<String>,
}

struct DecodedHistory {
    rows: Vec<StoredPoint>,
    malformed_rows: usize,
    unsupported_schema_rows: usize,
    nonmonotonic_rows: usize,
}

enum RowError {
    Malformed,
    Unsupported,
}

/// Read a provider's retained observations without ever mutating the cache.
pub fn read_history(
    work_root: &Path,
    provider: &str,
    since: i64,
    until: Option<i64>,
) -> QuotaHistoryRead {
    let Some(provider) = provider_name(provider) else {
        return empty_read(HistorySourceState::UnknownProvider);
    };
    let path = history_path(work_root, provider);
    let body = match history_store::read_history_body(&path) {
        Ok(Some(body)) => body,
        Ok(None) => return empty_read(HistorySourceState::Missing),
        Err(_) => return empty_read(HistorySourceState::Unreadable),
    };
    let decoded = decode_history(&body);
    let source = source_state(&decoded);
    let points = points_from_rows(decoded.rows)
        .into_iter()
        .filter(|point| in_bounds(point.observed_at, since, until))
        .collect();
    QuotaHistoryRead {
        points,
        diagnostics: QuotaHistoryDiagnostics {
            source,
            malformed_rows: decoded.malformed_rows,
            unsupported_schema_rows: decoded.unsupported_schema_rows,
            nonmonotonic_rows: decoded.nonmonotonic_rows,
        },
    }
}

/// Best-effort successful-poll writer. Callers keep provider success semantics.
pub(crate) fn record_successful_observation(
    work_root: &Path,
    provider: &str,
    quota: &ProviderQuota,
) -> Result<()> {
    history_store::record_successful_observation(work_root, provider, quota)
}

fn decode_history(body: &str) -> DecodedHistory {
    let mut decoded = DecodedHistory {
        rows: Vec::new(),
        malformed_rows: 0,
        unsupported_schema_rows: 0,
        nonmonotonic_rows: 0,
    };
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        match parse_row(line) {
            Ok(row)
                if decoded
                    .rows
                    .last()
                    .is_none_or(|last| row.observed_at > last.observed_at) =>
            {
                decoded.rows.push(row);
            }
            Ok(_) => decoded.nonmonotonic_rows += 1,
            Err(RowError::Malformed) => decoded.malformed_rows += 1,
            Err(RowError::Unsupported) => decoded.unsupported_schema_rows += 1,
        }
    }
    decoded
}

fn parse_row(line: &str) -> std::result::Result<StoredPoint, RowError> {
    let value: serde_json::Value = serde_json::from_str(line).map_err(|_| RowError::Malformed)?;
    match value.get("schema_version") {
        Some(version) if version.as_u64() == Some(1) => {}
        Some(version) if version.as_u64().is_some() => return Err(RowError::Unsupported),
        _ => return Err(RowError::Malformed),
    }
    let row: StoredPoint = serde_json::from_value(value).map_err(|_| RowError::Malformed)?;
    if row.observed_at <= 0 {
        return Err(RowError::Malformed);
    }
    Ok(sanitize_stored(row))
}

fn sanitize_stored(row: StoredPoint) -> StoredPoint {
    let quota = cache::sanitize(ProviderQuota {
        observed_at: row.observed_at,
        windows: row.windows,
        plan: row.plan,
        error: None,
    });
    StoredPoint {
        schema_version: 1,
        observed_at: quota.observed_at,
        windows: quota.windows,
        plan: quota.plan,
    }
}

fn stored_observation(quota: &ProviderQuota) -> StoredPoint {
    sanitize_stored(StoredPoint {
        schema_version: 1,
        observed_at: quota.observed_at,
        windows: quota.windows.clone(),
        plan: quota.plan.clone(),
    })
}

fn points_from_rows(rows: Vec<StoredPoint>) -> Vec<QuotaHistoryPoint> {
    let mut points: Vec<QuotaHistoryPoint> = Vec::with_capacity(rows.len());
    for row in rows {
        let continuity = points
            .last()
            .map_or(HistoryContinuity::Initial, |previous| {
                continuity_from_windows(&previous.windows, &row.windows)
            });
        points.push(QuotaHistoryPoint {
            observed_at: row.observed_at,
            windows: row.windows,
            plan: row.plan,
            continuity,
        });
    }
    points
}

fn continuity(previous: &StoredPoint, next: &StoredPoint) -> HistoryContinuity {
    continuity_from_windows(&previous.windows, &next.windows)
}

fn continuity_from_windows(previous: &[QuotaWindow], next: &[QuotaWindow]) -> HistoryContinuity {
    let mut compared = false;
    let mut reset = false;
    let mut unknown = false;
    for kind in [WindowKind::FiveHour, WindowKind::SevenDay] {
        match (window_of(previous, kind), window_of(next, kind)) {
            (None, None) => {}
            (Some(previous), Some(next)) => {
                compared = true;
                match (previous.resets_at, next.resets_at) {
                    (Some(left), Some(right)) if left != right => reset = true,
                    (Some(_), Some(_)) if next.used_percent < previous.used_percent => {
                        unknown = true;
                    }
                    (Some(_), Some(_)) => {}
                    _ => unknown = true,
                }
            }
            _ => unknown = true,
        }
    }
    if unknown || !compared {
        HistoryContinuity::Unknown
    } else if reset {
        HistoryContinuity::Reset
    } else {
        HistoryContinuity::SameReset
    }
}

fn window_of(windows: &[QuotaWindow], kind: WindowKind) -> Option<&QuotaWindow> {
    windows.iter().find(|window| window.kind == kind)
}

fn in_bounds(observed_at: i64, since: i64, until: Option<i64>) -> bool {
    observed_at >= since && until.is_none_or(|upper| observed_at <= upper)
}

fn source_state(decoded: &DecodedHistory) -> HistorySourceState {
    let rejected =
        decoded.malformed_rows + decoded.unsupported_schema_rows + decoded.nonmonotonic_rows;
    if !decoded.rows.is_empty() {
        HistorySourceState::Read
    } else if rejected > 0 {
        HistorySourceState::Corrupt
    } else {
        HistorySourceState::Empty
    }
}

fn empty_read(source: HistorySourceState) -> QuotaHistoryRead {
    QuotaHistoryRead {
        points: Vec::new(),
        diagnostics: QuotaHistoryDiagnostics {
            source,
            malformed_rows: 0,
            unsupported_schema_rows: 0,
            nonmonotonic_rows: 0,
        },
    }
}

fn provider_name(provider: &str) -> Option<&'static str> {
    history_store::PROVIDERS
        .iter()
        .copied()
        .find(|name| *name == provider)
}

fn provider_index(provider: &str) -> Option<usize> {
    history_store::PROVIDERS
        .iter()
        .position(|name| *name == provider)
}

fn history_path(work_root: &Path, provider: &str) -> PathBuf {
    cache::quota_dir(work_root)
        .join("history")
        .join(format!("{provider}.jsonl"))
}

fn history_path_in(dir: &Path, provider: &str) -> PathBuf {
    dir.join(format!("{provider}.jsonl"))
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "history_store_tests.rs"]
mod store_tests;
