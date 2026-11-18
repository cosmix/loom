//! Dispute identity is persisted before mutation; its receipt shares the stage write.
use super::Response;
use crate::models::{
    dispute::{DisputeRequest, CLOSED_MARKER},
    stage::Stage,
};
use anyhow::{bail, Result};
use std::path::Path;

pub(super) fn existing(stage: &Stage, key: Option<&str>) -> Result<Option<Response>> {
    let Some(key) = key else {
        return Ok(None);
    };
    if let Some(id) = stage.tally.request_receipts.get(key) {
        return Ok(Some(Response::DisputeCreated { id: *id }));
    }
    if stage.tally.request_receipts.len() >= 1024 {
        bail!("Stage request receipt capacity exhausted; refusing to evict replay protection");
    }
    Ok(None)
}

const PUBLISHED_PREFIX: &str = "dispute-published:";

/// Name dispute `id` in the stage, in the stage write that spends its budget,
/// with the caller's key when it sent one. Request keys pass `validate_id`,
/// which rejects ':', so the published name collides with none.
pub(super) fn remember(stage: &mut Stage, key: Option<&str>, id: u32) {
    let published = format!("{PUBLISHED_PREFIX}{id}");
    stage.tally.request_receipts.insert(published, id);
    if let Some(key) = key {
        stage.tally.request_receipts.insert(key.to_owned(), id);
    }
}

/// Whether request `id` is one no filing of `stage` published: a crash stopped
/// its filing before the stage write. A stage that names no dispute at all did
/// not reach adjudication through a filing, so nothing tells its requests
/// apart and none is called orphaned.
pub(crate) fn is_orphaned(stage: &Stage, id: u32) -> bool {
    let receipts = &stage.tally.request_receipts;
    !receipts.contains_key(&format!("{PUBLISHED_PREFIX}{id}"))
        && receipts.keys().any(|key| key.starts_with(PUBLISHED_PREFIX))
}

/// Remove the request a failed filing wrote, with its directory and receipt, and
/// nothing else. The caller holds the per-stage dispute lock.
pub(super) fn withdraw(directory: &Path, id: u32, key: Option<&str>) {
    let dispute = directory.join(id.to_string());
    let receipt = key.map(|key| directory.join("requests").join(format!("{key}.json")));
    let removed = std::fs::remove_file(dispute.join("request.md"))
        .and_then(|()| std::fs::remove_dir(&dispute))
        .and_then(|()| receipt.map_or(Ok(()), std::fs::remove_file));
    if let Err(error) = removed {
        tracing::warn!(dispute = id, %error, "could not withdraw a failed dispute request");
    }
}

pub(super) fn write_request(
    directory: &Path,
    record: DisputeRequest,
    key: Option<&str>,
) -> Result<u32> {
    let Some(key) = key else {
        return super::dispute_store::write_request(directory, record);
    };
    crate::validation::validate_id(key)?;
    let receipts = directory.join("requests");
    std::fs::create_dir_all(&receipts)?;
    let path = receipts.join(format!("{key}.json"));
    let reserved: Option<u32> = if path.exists() {
        Some(serde_json::from_str(&std::fs::read_to_string(&path)?)?)
    } else {
        None
    };
    // A settled request stays settled: this filing cannot reopen it.
    let reusable = reserved.filter(|id| {
        let marker = directory.join(id.to_string()).join(CLOSED_MARKER);
        std::fs::symlink_metadata(marker).is_err()
    });
    let id = match reusable {
        Some(id) => id,
        None => {
            let id = super::dispute_store::allocate_request(directory)?;
            crate::fs::locking::locked_write(&path, &serde_json::to_string(&id)?)?;
            id
        }
    };
    super::dispute_store::write_reserved_request(directory, record, id, true)?;
    Ok(id)
}
