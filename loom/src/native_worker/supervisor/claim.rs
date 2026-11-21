//! What a supervisor checks and takes before it starts a boundary: the claim
//! on the invocation, its admitted state, its id and the stage's build cache.

use std::os::fd::AsRawFd;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::fs::safe_fs::{safe_create_new_in_workdir, safe_open_dirfd};
use crate::native_worker::ids::validate_invocation_id;
use crate::native_worker::model::{NativeEvent, NativeManifest};
use crate::native_worker::store;

/// The stage's build cache, created before the boundary binds it. `dir` is
/// `<work_dir>/native-workers/<stage>/<invocation>` (`store::invocation_dir`).
pub(super) fn stage_cache_of(dir: &Path, stage_id: &str) -> Result<crate::stage_cache::StageCache> {
    let work_dir = dir
        .ancestors()
        .nth(3)
        .context("the invocation directory is not under a work directory")?;
    let cache = crate::verify::criteria::stage_cache_for(work_dir, stage_id)?;
    cache.ensure()?;
    Ok(cache)
}

/// The validated invocation id: the last component of `dir`.
pub(super) fn invocation_of(dir: &Path) -> Result<String> {
    let id = dir
        .file_name()
        .and_then(|name| name.to_str())
        .context("the invocation directory has no UTF-8 name")?;
    validate_invocation_id(id)
        .map_err(|error| anyhow::anyhow!("invalid invocation id: {error}"))?;
    Ok(id.to_string())
}

/// Creates `supervisor.json` exclusively: whoever creates it owns the
/// invocation, so a second supervisor (or a daemon tombstone) refuses here.
pub(super) fn claim(dir: &Path) -> Result<()> {
    let pid = std::process::id();
    let start = crate::process::process_start_time(pid)
        .context("cannot read the supervisor's process start time")?;
    let body = serde_json::to_vec(&serde_json::json!({
        "pid": pid as i32,
        "start": start.to_string(),
    }))?;
    let fd = safe_open_dirfd(dir)?;
    safe_create_new_in_workdir(fd.as_raw_fd(), Path::new("supervisor.json"), &body)
        .context("supervisor.json already exists or cannot be created")
}

/// The manifest of a record that is exactly `[Admitted]` with no boundary;
/// `None` when it already holds a `Terminal` event.
pub(super) fn admitted_manifest(dir: &Path) -> Result<Option<NativeManifest>> {
    let manifest = store::read_manifest(dir).context("manifest.json")?;
    let events = store::read_events(dir).context("events.jsonl")?;
    if events
        .iter()
        .any(|record| matches!(record.event, NativeEvent::Terminal { .. }))
    {
        return Ok(None);
    }
    if events.len() != 1 || events[0].event != NativeEvent::Admitted {
        bail!("invocation is not in the admitted state");
    }
    if store::read_boundary(dir)
        .context("boundary.json")?
        .is_some()
    {
        bail!("boundary.json already exists");
    }
    Ok(Some(manifest))
}
