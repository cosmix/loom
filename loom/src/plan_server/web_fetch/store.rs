//! `web-fetch.json` in the runtime directory: written through a temp file and a rename, read
//! bounded, never through a symlink. The temp file is synced before the rename, so a crash
//! leaves the old policy or the new one, never an empty file.

use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{Context, Result};

use crate::fs::safe_fs::{
    safe_remove_in_workdir, safe_rename_in_workdir, safe_write_with_mode_in_workdir,
};
use crate::fs::safe_read::{is_not_found, read_bounded_at};
use crate::plan_session::web_fetch::FetchPolicy;
use crate::planning::runtime::{open_runtime_dir, RuntimePaths};

const FILE: &str = "web-fetch.json";
const TEMP: &str = ".web-fetch.json.tmp";
/// A policy is at most `MAX_POLICY_BYTES` serialized; the rest is room for a hand edit.
const MAX_BYTES: usize = 32 * 1024;
/// Private to the user, as the rest of the runtime directory.
const FILE_MODE: libc::mode_t = 0o600;

/// The saved policy, its entries validated again; `None` when nothing was saved.
pub(super) fn read(runtime: &RuntimePaths) -> Result<Option<FetchPolicy>> {
    let dir = match open_runtime_dir(runtime, false) {
        Ok(dir) => dir,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let bytes = match read_bounded_at(&dir, Path::new(FILE), MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let saved: FetchPolicy =
        serde_json::from_slice(&bytes).context("cannot parse web-fetch.json")?;
    let policy = FetchPolicy::validated(saved.mode, &saved.domains)
        .map_err(|why| anyhow::anyhow!("web-fetch.json: {why}"))?;
    Ok(Some(policy))
}

pub(super) fn write(runtime: &RuntimePaths, policy: &FetchPolicy) -> Result<()> {
    let bytes = serde_json::to_vec(policy).context("cannot serialise the web fetch settings")?;
    let dir = open_runtime_dir(runtime, true)?;
    // A temp file a crashed writer left behind, or a symlink planted at its name, is unlinked
    // (never followed); none is the normal case, so the removal's error is not one. The write
    // opens without following a link and syncs before the rename.
    let _ = safe_remove_in_workdir(dir.as_raw_fd(), Path::new(TEMP));
    safe_write_with_mode_in_workdir(dir.as_raw_fd(), Path::new(TEMP), &bytes, FILE_MODE)
        .context("cannot write the web fetch settings")?;
    safe_rename_in_workdir(dir.as_raw_fd(), Path::new(TEMP), Path::new(FILE))
        .context("cannot publish the web fetch settings")
}
