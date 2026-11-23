//! The session configuration as it was before Codex ran, kept on disk in the runtime directory.
//!
//! A server that dies while Codex runs never compares an in-memory snapshot, so the next
//! server's recovery loads this file (`recover::check_baseline`). The runtime directory outlives
//! the session directory recovery removes, which is why the file lives here.

use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::config_guard::ConfigSnapshot;
use crate::fs::safe_fs::{safe_open_dirfd, safe_remove_in_workdir};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::fs::safe_replace::replace_private;
use crate::planning::runtime::RuntimePaths;

/// The file in the runtime directory holding the baseline.
const BASELINE_FILE: &str = "config-baseline.json";

/// Fail closed above this size: a truncated read would parse as a different baseline.
const MAX_BASELINE_BYTES: usize = 32 * 1024 * 1024;

/// Serialises `save` and `settle` inside this server: recovery's compare-and-remove must not
/// interleave with a new run saving its baseline.
static GUARD: Mutex<()> = Mutex::new(());

/// The file's content: the snapshot plus a token unique to the run that saved it, so two runs
/// with an identical configuration still leave distinguishable baselines.
#[derive(Serialize, Deserialize)]
struct Saved {
    run: String,
    snapshot: ConfigSnapshot,
}

/// The baseline file's bytes as one read saw them; `settle` acts only while the file still
/// holds exactly these bytes.
pub(super) struct Loaded {
    raw: Vec<u8>,
}

impl Loaded {
    /// The snapshot; an error when the file is not a valid baseline.
    pub(super) fn snapshot(&self) -> Result<ConfigSnapshot> {
        serde_json::from_slice::<Saved>(&self.raw)
            .map(|saved| saved.snapshot)
            .context("the configuration baseline is not valid")
    }
}

/// How `settle` ended; the action ran in both cases.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Settled {
    /// The baseline was the one seen: the file is gone.
    Removed,
    /// Another run replaced (or removed) the baseline meanwhile: the file is left to that run.
    Kept,
}

fn guard() -> std::sync::MutexGuard<'static, ()> {
    GUARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Record `before` atomically under a fresh run token; a crash mid-write leaves the old file
/// or none.
pub(super) fn save(runtime: &RuntimePaths, before: &ConfigSnapshot) -> Result<()> {
    let saved = Saved {
        run: uuid::Uuid::new_v4().to_string(),
        snapshot: before.clone(),
    };
    let bytes = serde_json::to_vec(&saved).context("cannot encode the configuration baseline")?;
    let _held = guard();
    replace_private(&runtime.dir, Path::new(BASELINE_FILE), &bytes)
}

fn read_raw(runtime: &RuntimePaths) -> Result<Option<Vec<u8>>> {
    match read_bounded(&runtime.dir, Path::new(BASELINE_FILE), MAX_BASELINE_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

/// The saved baseline; `None` when none exists, an error when the file cannot be read.
pub(super) fn load(runtime: &RuntimePaths) -> Result<Option<Loaded>> {
    read_raw(runtime).map(|raw| raw.map(|raw| Loaded { raw }))
}

/// Run `act`, then remove the baseline, but only while the file is still what `seen` loaded
/// (`None`: it could not be read then and still cannot be). `act` always runs: a run that
/// started meanwhile does not cancel the check of the run it replaced, it only keeps its own
/// baseline file. A failed `act` keeps the baseline.
pub(super) fn settle(
    runtime: &RuntimePaths,
    seen: Option<&Loaded>,
    act: impl FnOnce() -> Result<()>,
) -> Result<Settled> {
    let _held = guard();
    let unchanged = match (seen, read_raw(runtime)) {
        (Some(seen), Ok(Some(now))) => seen.raw == now,
        (None, Err(_)) => true,
        _ => false,
    };
    act()?;
    if !unchanged {
        return Ok(Settled::Kept);
    }
    if let Err(error) = clear(runtime) {
        tracing::warn!(%error, "cannot remove the configuration baseline");
    }
    Ok(Settled::Removed)
}

/// Remove the baseline; a missing file is fine.
pub(super) fn clear(runtime: &RuntimePaths) -> Result<()> {
    let dir = match safe_open_dirfd(&runtime.dir) {
        Ok(dir) => dir,
        Err(error) if is_not_found(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    safe_remove_in_workdir(dir.as_raw_fd(), Path::new(BASELINE_FILE))
}
