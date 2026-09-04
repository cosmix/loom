//! Plain data types and pure formatting helpers for provider quota tracking.
//!
//! Nothing here touches the filesystem or the network - see [`super::cache`],
//! [`super::claude`], and [`super::codex`] for the I/O.

use serde::{Deserialize, Serialize};

/// Cached quota state for both providers, as read from disk.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuotaSnapshot {
    pub claude: Option<ProviderQuota>,
    pub codex: Option<ProviderQuota>,
}

/// One provider's last-known quota, as last written by the poller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderQuota {
    /// Epoch seconds of the last successful poll (unchanged when a poll fails).
    pub observed_at: i64,
    /// Zero to two windows, five-hour first.
    pub windows: Vec<QuotaWindow>,
    /// Codex `planType`; `None` for Claude.
    pub plan: Option<String>,
    /// Last poll failure, already flattened with `inline_safe`; `None` after
    /// a success.
    pub error: Option<String>,
}

/// One usage window (e.g. "5 hours" or "7 days") within a provider's quota.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub kind: WindowKind,
    /// Clamped to 0..=100; `NaN` or infinity is rejected at parse time.
    pub used_percent: f64,
    /// Epoch seconds; `None` when the provider gave no reset time.
    pub resets_at: Option<i64>,
}

/// Which rolling window a [`QuotaWindow`] describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowKind {
    FiveHour,
    SevenDay,
}

impl WindowKind {
    /// Short display label used by both renderers.
    pub fn label(self) -> &'static str {
        match self {
            WindowKind::FiveHour => "5h",
            WindowKind::SevenDay => "7d",
        }
    }
}

/// How stale a cached [`ProviderQuota`] is allowed to get before a renderer
/// should call it out rather than show it as current.
pub const STALE_AFTER_SECS: i64 = 600;

/// Bucket a usage percentage into the same green/yellow/red bands used for
/// context-window health, reusing the existing thresholds (60% / 90%).
pub fn quota_health(used_percent: f64) -> crate::orchestrator::monitor::ContextHealth {
    crate::orchestrator::monitor::context_health(used_percent.round() as u32, 100)
}

/// Format a duration in seconds the way a reset countdown should read: short
/// units below a day, `"<days>d<hours>h"` at or above it.
///
/// Negative input is treated as zero.
pub fn format_reset(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 86_400 {
        crate::utils::format_elapsed(seconds)
    } else {
        format!("{}d{}h", seconds / 86_400, (seconds % 86_400) / 3600)
    }
}

/// Render a reset time relative to `now`, or `None` when the provider gave no
/// reset time at all. Callers add their own prefix (e.g. `"· "` or
/// `"resets in "`).
pub fn reset_text(resets_at: Option<i64>, now: i64) -> Option<String> {
    let resets_at = resets_at?;
    if resets_at <= now {
        Some("now".to_string())
    } else {
        Some(format_reset(resets_at - now))
    }
}

/// Seconds since a snapshot was observed, clamped to zero so clock skew
/// between the poller and the reader never reports a negative age.
pub fn age_secs(observed_at: i64, now: i64) -> i64 {
    (now - observed_at).max(0)
}

/// Drop non-finite percentages and clamp the rest to the valid 0..=100 range.
///
/// Shared by [`super::claude`] and [`super::codex`] so a malformed or
/// out-of-range value from either provider is handled identically.
pub fn clamp_percent(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 100.0))
}

/// Normalize a timestamp that may be in epoch seconds or epoch milliseconds.
///
/// Any value larger than a seconds-scale timestamp could plausibly be (year
/// ~5138 in seconds) is assumed to be milliseconds and divided down.
pub fn normalize_epoch(value: i64) -> i64 {
    const MILLISECOND_THRESHOLD: i64 = 100_000_000_000;
    if value > MILLISECOND_THRESHOLD {
        value / 1000
    } else {
        value
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
