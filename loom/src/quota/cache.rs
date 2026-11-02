//! On-disk cache for provider quota snapshots: `<work_root>/quota/<provider>.json`.
//!
//! Writers (the quota poller) and readers (`loom status`, the web dashboard)
//! run as separate processes, so every read tolerates a missing, oversized,
//! symlinked, or malformed file by returning `None` rather than propagating
//! an error - a bad cache entry must never take down status reporting.

use crate::context::untrusted::inline_safe;
use crate::fs::cache_io::{atomic_write, create_dir, reject_symlink};
use crate::fs::safe_read::read_to_string_bounded;
use crate::quota::model::{ProviderQuota, QuotaWindow, WindowKind};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Cached files are never trusted past this size; a body at or above the cap
/// is treated the same as a corrupt file.
const MAX_CACHE_FILE_BYTES: usize = 64 * 1024;

/// The `quota/` directory under a work root.
pub fn quota_dir(work_root: &Path) -> PathBuf {
    work_root.join("quota")
}

/// The path a provider's cache file lives at.
pub fn provider_path(work_root: &Path, provider: &str) -> PathBuf {
    quota_dir(work_root).join(format!("{provider}.json"))
}

/// Write `quota` to `provider`'s cache file, replacing whatever was there.
///
/// Refuses to write through a symlink, and writes via a temp file + rename so
/// a concurrent reader never observes a partially written file.
pub fn write_provider(work_root: &Path, provider: &str, quota: &ProviderQuota) -> Result<()> {
    let dir = quota_dir(work_root);
    create_dir(&dir)?;

    let path = provider_path(work_root, provider);
    reject_symlink(&path)?;

    let content = serde_json::to_string_pretty(quota).context("failed to serialize quota")?;
    atomic_write(&path, &content)
}

/// Read and sanitize a provider's cached quota, or `None` when the file is
/// absent, oversized, unreadable, or fails to parse. Never deletes the file.
///
/// The read goes through `read_to_string_bounded`, which opens without
/// following a symlink and without blocking, so a FIFO or a symlink planted at
/// the cache path reads as `None` instead of stalling the 1 s status collector
/// or the token tracker.
pub fn read_provider(work_root: &Path, provider: &str) -> Option<ProviderQuota> {
    let root = quota_dir(work_root);
    let relative = PathBuf::from(format!("{provider}.json"));
    // `read_to_string_bounded` refuses a body over its limit; the cap here is
    // exclusive, so the limit is one byte below it.
    let body = read_to_string_bounded(&root, &relative, MAX_CACHE_FILE_BYTES - 1).ok()?;

    let quota: ProviderQuota = serde_json::from_str(&body).ok()?;
    Some(sanitize(quota))
}

/// Record a poll failure for `provider`, preserving whatever windows/plan the
/// last successful poll observed.
///
/// When there is no cache file yet, nothing is written: a provider with no
/// on-disk history has no windows worth showing, and writing a placeholder
/// (`observed_at: 0`) would instead render as a quota reading decades stale.
pub fn record_failure(work_root: &Path, provider: &str, error: &str) -> Result<()> {
    let Some(mut existing) = read_provider(work_root, provider) else {
        return Ok(());
    };
    existing.error = Some(inline_safe(error));
    write_provider(work_root, provider, &existing)
}

/// Drop non-finite windows, clamp percentages, keep at most one window per
/// [`WindowKind`] ordered five-hour first, and flatten `plan`/`error` through
/// [`inline_safe`] - the same hygiene a fresh poll result gets, applied again
/// here in case an older loom version wrote a less strict cache file.
pub(crate) fn sanitize(mut quota: ProviderQuota) -> ProviderQuota {
    let mut five_hour: Option<QuotaWindow> = None;
    let mut seven_day: Option<QuotaWindow> = None;

    for window in quota.windows {
        if !window.used_percent.is_finite() {
            continue;
        }
        let sanitized = QuotaWindow {
            kind: window.kind,
            used_percent: window.used_percent.clamp(0.0, 100.0),
            resets_at: window.resets_at,
        };
        match sanitized.kind {
            WindowKind::FiveHour => five_hour.get_or_insert(sanitized),
            WindowKind::SevenDay => seven_day.get_or_insert(sanitized),
        };
    }

    quota.windows = [five_hour, seven_day].into_iter().flatten().collect();
    quota.plan = quota.plan.map(|plan| inline_safe(&plan));
    quota.error = quota.error.map(|error| inline_safe(&error));
    quota
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
