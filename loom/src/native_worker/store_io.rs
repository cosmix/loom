//! Descriptor-anchored file helpers shared by the record store.

use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};

use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::fs::safe_read::{open_regular_no_follow, single_link_regular_file};
use crate::fs::safe_replace::publish_in_dir;
use crate::models::forward_receipt::locator::effective_uid;

use super::model::MAX_RECORD_BYTES;
use super::store::StoreError;

pub(super) fn io_err(error: anyhow::Error) -> StoreError {
    StoreError::UnsafePath(format!("{error:#}"))
}

pub(super) fn read_regular(
    dir: &Path,
    name: &str,
    cap: u64,
) -> Result<Option<Vec<u8>>, StoreError> {
    let Some(file) = open_regular_no_follow(dir, name, libc::O_RDONLY).map_err(io_err)? else {
        return Ok(None);
    };
    let metadata = file.metadata()?;
    if metadata.uid() != effective_uid() {
        return Err(StoreError::UnsafePath(format!(
            "{name}: owner differs from current uid"
        )));
    }
    if metadata.len() > cap {
        return Err(StoreError::TooLarge("record"));
    }
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(StoreError::TooLarge("record"));
    }
    Ok(Some(bytes))
}

pub(super) fn publish(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
    let fd = safe_open_dirfd(dir).map_err(io_err)?;
    publish_in_dir(&fd, OsStr::new(name), bytes, 0o600, true).map_err(io_err)
}

pub(super) fn open_append(dir: &Path) -> Result<File, StoreError> {
    let fd = safe_open_dirfd(dir).map_err(io_err)?;
    let file = File::from(
        open_safely(
            fd.as_raw_fd(),
            Path::new("events.jsonl"),
            libc::O_WRONLY | libc::O_APPEND | libc::O_NONBLOCK,
            0,
        )
        .map_err(io_err)?,
    );
    single_link_regular_file(&file).map_err(io_err)?;
    if file.metadata()?.uid() != effective_uid() {
        return Err(StoreError::UnsafePath(
            "events.jsonl: owner differs from current uid".into(),
        ));
    }
    Ok(file)
}

pub(super) fn read_json<T: DeserializeOwned>(
    dir: &Path,
    name: &str,
) -> Result<Option<T>, StoreError> {
    read_regular(dir, name, MAX_RECORD_BYTES)?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| StoreError::Corrupt(format!("{name}: {error}")))
        })
        .transpose()
}

pub(super) fn write_json<T: Serialize>(
    dir: &Path,
    name: &str,
    value: &T,
) -> Result<(), StoreError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| StoreError::Corrupt(format!("{name}: {error}")))?;
    publish(dir, name, &bytes)
}
