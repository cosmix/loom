//! Shared host observations used by the monitor and activity badge.

use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::fs::safe_fs::{open_safely, safe_create_dir_all_in_workdir, safe_open_dirfd};
use crate::process::{verify_process_identity, IdentityStatus, ProcessIdentity};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ForegroundObservation {
    pid: u32,
    started: Option<u64>,
    /// Every rise seen between two samples, added up: the tree's total falls
    /// when a process in it exits unreaped, and work after that still counts.
    cpu_ticks: u64,
    last_sample: u64,
    sampled_at: DateTime<Utc>,
    progress_at: DateTime<Utc>,
}

pub(super) struct CommandProgress {
    pub(super) pid: u32,
    pub(super) started: Option<u64>,
    pub(super) cpu_ticks: u64,
}

// CPU growth can extend a command beyond the stall line, but never past
// this wall deadline (or a larger explicit stage budget).
const FOREGROUND_HOLD_CEILING_SECS: u64 = 1560;

pub(super) fn within_window(baseline: DateTime<Utc>, budget: u64, now: DateTime<Utc>) -> bool {
    let ceiling = FOREGROUND_HOLD_CEILING_SECS.max(budget);
    let elapsed = now.signed_duration_since(baseline).to_std();
    budget > 0 && elapsed.is_ok_and(|age| age.as_secs() < ceiling)
}

impl ForegroundObservation {
    pub(super) fn advance(
        previous: Option<Self>,
        progress: CommandProgress,
        baseline: DateTime<Utc>,
        budget: u64,
        now: DateTime<Utc>,
    ) -> Option<Self> {
        let duration = chrono::Duration::try_seconds(i64::try_from(budget).ok()?)?;
        let initial = now.min(baseline.checked_add_signed(duration)?);
        if let Some(mut old) =
            previous.filter(|old| old.pid == progress.pid && old.started == progress.started)
        {
            // A reader that took its sample before the last one published.
            if now < old.sampled_at {
                return Some(old);
            }
            if progress.cpu_ticks > old.last_sample {
                let rise = progress.cpu_ticks - old.last_sample;
                old.cpu_ticks = old.cpu_ticks.saturating_add(rise);
                old.progress_at = old.progress_at.max(now);
            }
            old.last_sample = progress.cpu_ticks;
            old.sampled_at = now;
            return Some(old);
        }
        Some(Self {
            pid: progress.pid,
            started: progress.started,
            cpu_ticks: progress.cpu_ticks,
            last_sample: progress.cpu_ticks,
            sampled_at: now,
            progress_at: initial,
        })
    }

    pub(super) fn holds(&self, baseline: DateTime<Utc>, budget: u64, now: DateTime<Utc>) -> bool {
        within_window(baseline, budget, now)
            && now
                .signed_duration_since(self.progress_at)
                .num_seconds()
                .max(0)
                < i64::try_from(budget).unwrap_or(i64::MAX)
    }
}

pub(super) fn record(
    work: &Path,
    session_id: &str,
    progress: CommandProgress,
    baseline: DateTime<Utc>,
    budget: u64,
    now: DateTime<Utc>,
) -> Result<ForegroundObservation> {
    let name = checkpoint_name(session_id)?;
    let (root, dir) = checkpoint_directory(work)?;
    let lock = std::fs::File::from(dir.try_clone()?);
    let publish = match lock.try_lock_exclusive() {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(error) => return Err(error.into()),
    };
    let path = Path::new("heartbeat/command-progress").join(&name);
    let previous = match crate::fs::safe_read::read_bounded_at(&root, &path, 4096) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
        Err(error) if crate::fs::safe_read::is_not_found(&error) => None,
        Err(error) => return Err(error),
    };
    let identity = ProcessIdentity {
        pid: progress.pid,
        start_time: progress.started,
    };
    let observation = ForegroundObservation::advance(previous, progress, baseline, budget, now)
        .ok_or_else(|| anyhow::anyhow!("invalid foreground budget"))?;
    ensure!(
        identity.start_time.is_some()
            && verify_process_identity(identity) == IdentityStatus::VerifiedAlive,
        "foreground identity changed"
    );
    if publish {
        crate::fs::safe_replace::publish_in_dir(
            &dir,
            std::ffi::OsStr::new(&name),
            &serde_json::to_vec(&observation)?,
            0o600,
            true,
        )?;
    }
    Ok(observation)
}

/// The checkpoint recorded for the command `pid` of this session, if any.
pub(super) fn recorded(work: &Path, session_id: &str, pid: u32) -> Option<ForegroundObservation> {
    let name = checkpoint_name(session_id).ok()?;
    let path = Path::new("heartbeat/command-progress").join(name);
    let root = safe_open_dirfd(work).ok()?;
    let bytes = crate::fs::safe_read::read_bounded_at(&root, &path, 4096).ok()?;
    serde_json::from_slice::<ForegroundObservation>(&bytes)
        .ok()
        .filter(|observation| observation.pid == pid)
}

fn checkpoint_name(session_id: &str) -> Result<String> {
    ensure!(
        !session_id.is_empty()
            && session_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid session ID"
    );
    Ok(format!("{session_id}.json"))
}

fn checkpoint_directory(work: &Path) -> Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd)> {
    let root = safe_open_dirfd(work)?;
    let directory = Path::new("heartbeat/command-progress");
    safe_create_dir_all_in_workdir(root.as_raw_fd(), directory, 0o700)?;
    let dir = open_safely(
        root.as_raw_fd(),
        directory,
        libc::O_DIRECTORY | libc::O_RDONLY,
        0,
    )?;
    Ok((root, dir))
}

#[cfg(test)]
#[path = "foreground_progress_tests.rs"]
mod tests;
