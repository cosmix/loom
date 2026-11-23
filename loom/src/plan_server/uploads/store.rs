//! Writing the upload store `<runtime>/attachments`: content-addressed files, a sidecar each,
//! a total-size cap, `touch` and the expiry sweep.
//!
//! The directory is 0700 and opened without following symlinks; every file is written at 0600
//! through `safe_replace::publish_in_dir` (a temp file renamed into place, then a directory
//! sync). One process-wide lock orders the cap check with the write, `touch` and the sweep.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::fs::safe_fs::{safe_create_dir_all, safe_open_dirfd, safe_remove_in_workdir};
use crate::fs::safe_read::is_regular_no_follow;
use crate::fs::safe_replace::{is_publish_temp, publish_in_dir};
use crate::plan_session::attachments::stored::{
    self, extension, sidecar_name, Sidecar, ID_LEN, MEDIA_TYPES,
};
use crate::plan_session::attachments::FileMeta;
use crate::planning::runtime::ATTACHMENTS_DIR;

/// The stored files may total this many bytes; an upload past it is refused.
pub(super) const MAX_STORE_BYTES: u64 = 256 * 1024 * 1024;

/// A file and its sidecar are kept this long after their `stored_at`.
pub(super) const KEEP_SECONDS: u64 = 30 * 24 * 60 * 60;

/// Mode of every stored file and sidecar.
const FILE_MODE: u32 = 0o600;

static STORE_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    STORE_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) enum StoreError {
    /// Storing the file would take the store past [`MAX_STORE_BYTES`].
    Full,
    Io(anyhow::Error),
}

impl From<anyhow::Error> for StoreError {
    fn from(error: anyhow::Error) -> Self {
        StoreError::Io(error)
    }
}

/// Seconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Store `body` under the first [`ID_LEN`] hex characters of its sha256. The first media type
/// of a content id wins, preserving the file and its extension. Reusing a file needs no room
/// under the cap, and the sidecar keeps the latest upload's `name` and `stored_at`.
pub(super) fn put(
    runtime_dir: &Path,
    name: &str,
    media_type: &str,
    body: &[u8],
    now: u64,
) -> Result<FileMeta, StoreError> {
    put_within(runtime_dir, name, media_type, body, now, MAX_STORE_BYTES)
}

/// [`put`] with the store cap as a parameter, so a test can reach it without 256 MiB of data.
pub(super) fn put_within(
    runtime_dir: &Path,
    name: &str,
    media_type: &str,
    body: &[u8],
    now: u64,
    cap: u64,
) -> Result<FileMeta, StoreError> {
    let ext = extension(media_type).context("media type is not stored")?;
    let _lock = lock();
    safe_create_dir_all(runtime_dir, Path::new(ATTACHMENTS_DIR), 0o700)?;
    let dir = runtime_dir.join(ATTACHMENTS_DIR);
    let dirfd = safe_open_dirfd(&dir)?;
    let id = crate::utils::sha256_hex(body)[..ID_LEN].to_owned();
    let existing = stored::locate(&dir, &id);
    let data_name = format!("{id}.{ext}");
    let present = existing.is_some() || is_regular_no_follow(&dir.join(&data_name));
    if !present {
        let fits = || -> Result<bool> {
            let used = stored_bytes(&dir).context("cannot measure the attachment store")?;
            Ok(used.saturating_add(body.len() as u64) <= cap)
        };
        if !fits()? {
            sweep_locked(&dir, &dirfd, now);
            if !fits()? {
                return Err(StoreError::Full);
            }
        }
        publish_in_dir(&dirfd, OsStr::new(&data_name), body, FILE_MODE, true)
            .with_context(|| format!("cannot write {data_name}"))?;
    }
    let sidecar = Sidecar {
        name: name.to_owned(),
        media_type: existing.map_or_else(|| media_type.to_owned(), |(_, stored)| stored.media_type),
        bytes: body.len() as u64,
        stored_at: now,
    };
    write_sidecar(&dirfd, &id, &sidecar)?;
    Ok(FileMeta {
        id,
        name: sidecar.name,
        media_type: sidecar.media_type,
        bytes: sidecar.bytes,
    })
}

/// Refresh the `stored_at` of the stored file `id` under `runtime_dir` to now, so a file that an
/// unfinished draft still names outlives the expiry sweep. `Ok(false)` for an id of the wrong
/// shape or with no stored file; an error only when the sidecar cannot be rewritten.
pub(crate) fn touch(runtime_dir: &Path, id: &str) -> io::Result<bool> {
    touch_at(runtime_dir, id, now())
}

pub(super) fn touch_at(runtime_dir: &Path, id: &str, now: u64) -> io::Result<bool> {
    let _lock = lock();
    let dir = runtime_dir.join(ATTACHMENTS_DIR);
    let Some((_, mut sidecar)) = stored::locate(&dir, id) else {
        return Ok(false);
    };
    sidecar.stored_at = now;
    let rewrite = || -> Result<()> { write_sidecar(&safe_open_dirfd(&dir)?, id, &sidecar) };
    rewrite().map_err(io::Error::other)?;
    Ok(true)
}

fn write_sidecar(dirfd: &OwnedFd, id: &str, sidecar: &Sidecar) -> Result<()> {
    let json = serde_json::to_vec(sidecar).context("cannot encode the sidecar")?;
    let name = sidecar_name(id);
    publish_in_dir(dirfd, OsStr::new(&name), &json, FILE_MODE, true)
        .with_context(|| format!("cannot write {name}"))
}

/// The bytes of the regular files in `dir` (temp files included).
fn stored_bytes(dir: &Path) -> io::Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let meta = entry?.metadata()?;
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

/// What one sweep did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Swept {
    pub(super) removed: usize,
    pub(super) failed: usize,
}

/// Remove the files of `dir` whose stamp is more than [`KEEP_SECONDS`] before `now`: a stored
/// file and its sidecar share the sidecar's `stored_at`; a file with no readable sidecar and a
/// temp file are aged by their modification time; anything else is left alone. A missing
/// directory is an empty store; a file that will not go is counted and logged, never an error.
pub(super) fn sweep(dir: &Path, now: u64) -> Swept {
    let _lock = lock();
    // The dirfd refuses a symlinked store directory, and every removal goes through it.
    let Ok(dirfd) = safe_open_dirfd(dir) else {
        return Swept::default();
    };
    sweep_locked(dir, &dirfd, now)
}

fn sweep_locked(dir: &Path, dirfd: &OwnedFd, now: u64) -> Swept {
    let mut swept = Swept::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return swept;
    };
    let mut stamps: HashMap<String, Option<u64>> = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
        if !is_file {
            continue;
        }
        let stamp = if is_publish_temp(name.as_bytes()) {
            None
        } else if is_stored(&name) {
            let id = name.split('.').next().unwrap_or("").to_owned();
            *stamps
                .entry(id.clone())
                .or_insert_with(|| stored::read_sidecar(dir, &id).map(|sidecar| sidecar.stored_at))
        } else {
            continue;
        };
        let stamp = stamp.or_else(|| modified(&entry)).unwrap_or(now);
        if now.saturating_sub(stamp) <= KEEP_SECONDS {
            continue;
        }
        match safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new(&name)) {
            Ok(()) => swept.removed += 1,
            Err(error) => {
                swept.failed += 1;
                tracing::warn!("cannot remove expired attachment {name}: {error}");
            }
        }
    }
    swept
}

/// A store file: `<id>.<ext>` or `<id>.json` for a valid id.
fn is_stored(name: &str) -> bool {
    let Some((id, ext)) = name.split_once('.') else {
        return false;
    };
    let stored_ext = || {
        MEDIA_TYPES
            .iter()
            .any(|media| extension(media) == Some(ext))
    };
    stored::valid_id(id) && (ext == "json" || stored_ext())
}

fn modified(entry: &fs::DirEntry) -> Option<u64> {
    let time = entry.metadata().ok()?.modified().ok()?;
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}
