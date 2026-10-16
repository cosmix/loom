//! Dispute identity is persisted before mutation; its receipt shares the stage write.
use super::Response;
use crate::models::{dispute::DisputeRequest, stage::Stage};
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

pub(super) fn remember(stage: &mut Stage, key: Option<&str>, id: u32) {
    if let Some(key) = key {
        stage.tally.request_receipts.insert(key.to_owned(), id);
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
    let id = if path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&path)?)?
    } else {
        let id = super::dispute_store::allocate_request(directory)?;
        crate::fs::locking::locked_write(&path, &serde_json::to_string(&id)?)?;
        id
    };
    super::dispute_store::write_reserved_request(directory, record, id, true)?;
    Ok(id)
}
