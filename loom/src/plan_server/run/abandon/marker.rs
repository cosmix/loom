//! The abandon marker `<runtime dir>/abandon/<slug>.json`: an abandon of that plan began on
//! this server. Every access goes through the no-follow primitives of `crate::fs::safe_fs`.

use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::fs::safe_fs::{
    open_safely, safe_create_dir_all_in_workdir, safe_locked_write_in_workdir, safe_open_dirfd,
    safe_remove_in_workdir,
};
use crate::planning::runtime::RuntimePaths;

use super::plan::RunningPlan;

const DIR: &str = "abandon";
/// A marker is a few dozen bytes; anything larger is not one.
const MAX_BYTES: u64 = 4096;

#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    plan: String,
    started_at: String,
}

fn rel(plan: &RunningPlan) -> PathBuf {
    Path::new(DIR).join(format!("{}.json", plan.slug))
}

/// Record that an abandon of `plan` begins now (`abandon/` 0700, the file 0600).
pub(super) fn write(runtime: &RuntimePaths, plan: &RunningPlan) -> Result<()> {
    let dirfd = safe_open_dirfd(&runtime.dir)?;
    safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), Path::new(DIR), 0o700)?;
    let marker = Marker {
        plan: plan.source_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let body = serde_json::to_vec(&marker).context("encoding the abandon marker")?;
    safe_locked_write_in_workdir(dirfd.as_raw_fd(), &rel(plan), &body)
        .context("writing the abandon marker")
}

/// Remove `plan`'s marker; a missing one is not an error.
pub(super) fn remove(runtime: &RuntimePaths, plan: &RunningPlan) -> Result<()> {
    let dirfd = safe_open_dirfd(&runtime.dir)?;
    let dir_flags = libc::O_DIRECTORY | libc::O_RDONLY;
    if open_safely(dirfd.as_raw_fd(), Path::new(DIR), dir_flags, 0).is_err() {
        return Ok(());
    }
    safe_remove_in_workdir(dirfd.as_raw_fd(), &rel(plan))
}

/// [`remove`], logging a failure: the outcome it follows already happened.
pub(super) fn remove_logged(runtime: &RuntimePaths, plan: &RunningPlan) {
    if let Err(error) = remove(runtime, plan) {
        tracing::warn!("the abandon marker for {} stays: {error:#}", plan.slug);
    }
}

/// Whether a marker for `plan` exists and names it.
pub(super) fn names(runtime: &RuntimePaths, plan: &RunningPlan) -> bool {
    read(runtime, plan).is_ok_and(|marker| marker.plan == plan.source_string())
}

fn read(runtime: &RuntimePaths, plan: &RunningPlan) -> Result<Marker> {
    let dirfd = safe_open_dirfd(&runtime.dir)?;
    // O_NONBLOCK: a FIFO planted in place of the marker must not stall the server.
    let flags = libc::O_RDONLY | libc::O_NONBLOCK;
    let file = File::from(open_safely(dirfd.as_raw_fd(), &rel(plan), flags, 0)?);
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "the abandon marker is not a file"
    );
    let mut text = String::new();
    file.take(MAX_BYTES).read_to_string(&mut text)?;
    Ok(serde_json::from_str(&text)?)
}
