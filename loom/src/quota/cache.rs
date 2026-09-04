//! On-disk cache for provider quota snapshots: `<work_root>/quota/<provider>.json`.
//!
//! Writers (the quota poller) and readers (`loom status`, the web dashboard)
//! run as separate processes, so every read tolerates a missing, oversized,
//! symlinked, or malformed file by returning `None` rather than propagating
//! an error - a bad cache entry must never take down status reporting.

use crate::context::untrusted::inline_safe;
use crate::quota::model::{ProviderQuota, QuotaWindow, WindowKind};
use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// Cached files are never trusted past this size; a body at or above the cap
/// is treated the same as a corrupt file.
const MAX_CACHE_FILE_BYTES: u64 = 64 * 1024;

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
    create_quota_dir(&dir)?;

    let path = provider_path(work_root, provider);
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            bail!(
                "refusing to write quota cache through a symlink: {}",
                path.display()
            );
        }
    }

    let content = serde_json::to_string_pretty(quota).context("failed to serialize quota")?;
    atomic_write(&path, &content)
}

/// Read and sanitize a provider's cached quota, or `None` when the file is
/// absent, oversized, unreadable, or fails to parse. Never deletes the file.
pub fn read_provider(work_root: &Path, provider: &str) -> Option<ProviderQuota> {
    let path = provider_path(work_root, provider);
    let metadata = std::fs::metadata(&path).ok()?;
    if metadata.len() >= MAX_CACHE_FILE_BYTES {
        return None;
    }

    let file = File::open(&path).ok()?;
    let mut body = String::new();
    file.take(MAX_CACHE_FILE_BYTES)
        .read_to_string(&mut body)
        .ok()?;

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
fn sanitize(mut quota: ProviderQuota) -> ProviderQuota {
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

#[cfg(unix)]
fn create_quota_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("failed to create quota directory: {}", dir.display()))
}

#[cfg(not(unix))]
fn create_quota_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("failed to create quota directory: {}", dir.display()))
}

/// Crash-atomic write: `<path>.tmp` is written, fsynced, then renamed over
/// `path`; the containing directory is fsynced so the rename itself survives
/// a crash. Single-writer (the quota poller thread), so no advisory lock is
/// taken - readers already tolerate a torn or missing file.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp_path = PathBuf::from(tmp_os);

    write_temp_file(&tmp_path, content)?;

    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    if let Some(parent) = path.parent() {
        if let Ok(dir_file) = File::open(parent) {
            let _ = dir_file.sync_all();
        }
    }

    Ok(())
}

/// Writes and fsyncs `content` into `tmp_path`, unlinking any stale temp file first.
fn write_temp_file(tmp_path: &Path, content: &str) -> Result<()> {
    // Unlink whatever is at the temp path first rather than opening it with
    // `truncate(true)`: a planted symlink there would otherwise have its
    // target truncated instead of being replaced. This also clears a stale
    // temp file left behind by a prior crash. `create_new` below then fails
    // loudly instead of following anything recreated in between.
    if let Err(e) = std::fs::remove_file(tmp_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(e).with_context(|| {
                format!("failed to remove stale temp file: {}", tmp_path.display())
            });
        }
    }

    let tmp = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp_path)
        .with_context(|| format!("failed to open temp file: {}", tmp_path.display()))?;
    let mut writer = BufWriter::new(&tmp);
    writer
        .write_all(content.as_bytes())
        .with_context(|| format!("failed to write temp file: {}", tmp_path.display()))?;
    writer
        .flush()
        .with_context(|| format!("failed to flush temp file: {}", tmp_path.display()))?;
    drop(writer);
    tmp.sync_all()
        .with_context(|| format!("failed to sync temp file: {}", tmp_path.display()))?;

    Ok(())
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
