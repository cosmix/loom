//! Crash-safe bounded persistence for quota history rows.

use super::{
    continuity, decode_history, history_path_in, provider_index, stored_observation, StoredPoint,
};
use crate::fs::locking::locked_dir_update;
use crate::quota::cache;
use crate::quota::model::ProviderQuota;
use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;
const MAX_HISTORY_FILE_BYTES: u64 = MAX_HISTORY_BYTES as u64;
const HISTORY_RETENTION_SECS: i64 = 30 * 24 * 60 * 60;
pub(super) const PROVIDERS: [&str; 2] = ["claude", "codex"];

struct HistoryFile {
    provider: &'static str,
    rows: Vec<StoredPoint>,
    changed: bool,
}

pub(super) fn record_successful_observation(
    work_root: &Path,
    provider: &str,
    quota: &ProviderQuota,
) -> Result<()> {
    record_observation_at(
        work_root,
        provider,
        quota,
        chrono::Utc::now().timestamp(),
        MAX_HISTORY_BYTES,
    )
}

pub(super) fn record_observation_at(
    work_root: &Path,
    provider: &str,
    quota: &ProviderQuota,
    now: i64,
    max_bytes: usize,
) -> Result<()> {
    let Some(index) = provider_index(provider) else {
        bail!("unknown quota history provider");
    };
    let provider = PROVIDERS[index];
    if quota.observed_at <= 0 {
        eprintln!("quota: {provider}: ignored nonpositive history observation");
        return Ok(());
    }

    let dir = ensure_history_dir(work_root)?;
    locked_dir_update(&dir, || {
        let mut files = load_history_files(&dir)?;
        bound_history(&mut files, now, max_bytes)?;
        let observation = stored_observation(quota);

        if should_skip_observation(files[index].rows.last(), &observation, provider) {
            return write_history_files(&dir, &files);
        }
        if history_bytes(std::slice::from_ref(&observation))? > max_bytes {
            eprintln!("quota: {provider}: ignored oversized history observation");
            return write_history_files(&dir, &files);
        }

        files[index].rows.push(observation);
        files[index].changed = true;
        bound_history(&mut files, now, max_bytes)?;
        write_history_files(&dir, &files)
    })
}

pub(super) fn read_history_body(path: &Path) -> Result<Option<String>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| "failed to inspect quota history"),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        bail!("unsafe quota history file");
    }
    if metadata.len() > MAX_HISTORY_FILE_BYTES {
        bail!("oversized quota history file");
    }

    let mut body = String::new();
    File::open(path)?
        .take(MAX_HISTORY_FILE_BYTES + 1)
        .read_to_string(&mut body)?;
    if body.len() > MAX_HISTORY_BYTES {
        bail!("oversized quota history file");
    }
    Ok(Some(body))
}

fn should_skip_observation(last: Option<&StoredPoint>, next: &StoredPoint, provider: &str) -> bool {
    let Some(last) = last else {
        return false;
    };
    if next.observed_at <= last.observed_at {
        eprintln!("quota: {provider}: ignored nonmonotonic history observation");
        return true;
    }
    continuity(last, next) == super::HistoryContinuity::SameReset
        && last.windows == next.windows
        && last.plan == next.plan
}

fn ensure_history_dir(work_root: &Path) -> Result<PathBuf> {
    let quota_dir = cache::quota_dir(work_root);
    cache::create_quota_dir(&quota_dir)?;
    let dir = quota_dir.join("history");
    cache::reject_symlink(&dir)?;
    cache::create_quota_dir(&dir)?;
    let metadata = std::fs::symlink_metadata(&dir)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("quota history path is not a directory");
    }
    Ok(dir)
}

fn load_history_files(dir: &Path) -> Result<[HistoryFile; 2]> {
    Ok([
        load_history_file(dir, PROVIDERS[0])?,
        load_history_file(dir, PROVIDERS[1])?,
    ])
}

fn load_history_file(dir: &Path, provider: &'static str) -> Result<HistoryFile> {
    let path = history_path_in(dir, provider);
    let Some(body) = read_history_body(&path)? else {
        return Ok(HistoryFile {
            provider,
            rows: Vec::new(),
            changed: false,
        });
    };
    Ok(HistoryFile {
        provider,
        rows: decode_history(&body).rows,
        changed: false,
    })
}

fn bound_history(files: &mut [HistoryFile; 2], now: i64, max_bytes: usize) -> Result<()> {
    let cutoff = now.saturating_sub(HISTORY_RETENTION_SECS);
    for file in files.iter_mut() {
        let row_count = file.rows.len();
        file.rows.retain(|row| row.observed_at >= cutoff);
        file.changed |= file.rows.len() != row_count;
    }
    let mut bytes = total_history_bytes(files)?;
    let mut starts = [0, 0];
    while bytes > max_bytes {
        let Some(index) = oldest_file_index(files, starts) else {
            break;
        };
        let row = &files[index].rows[starts[index]];
        bytes = bytes.saturating_sub(history_bytes(std::slice::from_ref(row))?);
        starts[index] += 1;
    }
    for (file, start) in files.iter_mut().zip(starts) {
        if start > 0 {
            file.rows.drain(0..start);
            file.changed = true;
        }
    }
    Ok(())
}

fn oldest_file_index(files: &[HistoryFile; 2], starts: [usize; 2]) -> Option<usize> {
    let left = files[0].rows.get(starts[0]);
    let right = files[1].rows.get(starts[1]);
    match (left, right) {
        (Some(left), Some(right)) => Some(usize::from(right.observed_at < left.observed_at)),
        (Some(_), None) => Some(0),
        (None, Some(_)) => Some(1),
        (None, None) => None,
    }
}

fn total_history_bytes(files: &[HistoryFile; 2]) -> Result<usize> {
    let first = history_bytes(&files[0].rows)?;
    first
        .checked_add(history_bytes(&files[1].rows)?)
        .context("quota history size overflow")
}

fn history_bytes(rows: &[StoredPoint]) -> Result<usize> {
    rows.iter().try_fold(0_usize, |total, row| {
        let row_bytes = serde_json::to_string(row)?.len() + 1;
        total
            .checked_add(row_bytes)
            .context("quota history size overflow")
    })
}

fn write_history_files(dir: &Path, files: &[HistoryFile; 2]) -> Result<()> {
    for file in files {
        if file.changed {
            let path = history_path_in(dir, file.provider);
            cache::reject_symlink(&path)?;
            cache::atomic_write(&path, &serialize_rows(&file.rows)?)?;
        }
    }
    Ok(())
}

fn serialize_rows(rows: &[StoredPoint]) -> Result<String> {
    let mut body = String::new();
    for row in rows {
        body.push_str(&serde_json::to_string(row)?);
        body.push('\n');
    }
    Ok(body)
}
