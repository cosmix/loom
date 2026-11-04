//! Writing the upload store `<runtime>/attachments`: content-addressed files, a sidecar each,
//! a total-size cap, `touch` and the expiry sweep.
//!
//! Every write goes through `safe_fs`: the directory is 0700, a file is written under a temp
//! name opened `O_CREAT|O_EXCL|O_NOFOLLOW` at 0600 and renamed into place. One process-wide
//! lock orders the cap check with the write, `touch` and the sweep.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::fs::safe_fs::{
    open_safely, safe_create_dir_all, safe_open_dirfd, safe_remove_in_workdir,
    safe_rename_in_workdir,
};
use crate::plan_session::attachments::stored::{
    self, extension, sidecar_name, Sidecar, ID_LEN, MEDIA_TYPES,
};
use crate::plan_session::attachments::FileMeta;
use crate::planning::runtime::ATTACHMENTS_DIR;

/// The stored files may total this many bytes; an upload past it is refused.
pub(super) const MAX_STORE_BYTES: u64 = 256 * 1024 * 1024;

/// A file and its sidecar are kept this long after their `stored_at`.
pub(super) const KEEP_SECONDS: u64 = 30 * 24 * 60 * 60;

/// Prefix of the temp files a write renames into place.
const TEMP_PREFIX: &str = "tmp-";

static STORE_LOCK: Mutex<()> = Mutex::new(());
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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

/// Store `body` under the first [`ID_LEN`] hex characters of its sha256, with the extension of
/// `media_type`. A file already stored under that id is kept and counts nothing against the
/// cap; the sidecar is written anew either way, so `name` and `stored_at` are the latest
/// upload's.
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
    let id = hex::encode(Sha256::digest(body))[..ID_LEN].to_owned();
    let data_name = format!("{id}.{ext}");
    let present = fs::symlink_metadata(dir.join(&data_name)).is_ok_and(|meta| meta.is_file());
    if !present {
        let used = stored_bytes(&dir).context("cannot measure the attachment store")?;
        if used.saturating_add(body.len() as u64) > cap {
            return Err(StoreError::Full);
        }
        write_atomic(&dirfd, &data_name, body)?;
    }
    let sidecar = Sidecar {
        name: name.to_owned(),
        media_type: media_type.to_owned(),
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
    write_atomic(dirfd, &sidecar_name(id), &json)
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

/// Write `bytes` to a new temp file in the directory, then rename it to `name`.
fn write_atomic(dirfd: &OwnedFd, name: &str, bytes: &[u8]) -> Result<()> {
    let raw = dirfd.as_raw_fd();
    let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = format!("{TEMP_PREFIX}{}-{count}-{name}", std::process::id());
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL;
    let fd = open_safely(raw, Path::new(&temp), flags, 0o600)?;
    let mut file = File::from(fd);
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let placed = written
        .with_context(|| format!("cannot write {temp}"))
        .and_then(|()| safe_rename_in_workdir(raw, Path::new(&temp), Path::new(name)));
    if placed.is_err() {
        // Best effort: a leftover temp is swept by age.
        let _ = safe_remove_in_workdir(raw, Path::new(&temp));
    }
    placed
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
    let mut swept = Swept::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return swept;
    };
    let mut stamps: HashMap<String, Option<u64>> = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
        if !is_file || !is_ours(&name) {
            continue;
        }
        let id = name.split('.').next().unwrap_or("").to_owned();
        let stamp = *stamps
            .entry(id.clone())
            .or_insert_with(|| stored::read_sidecar(dir, &id).map(|sidecar| sidecar.stored_at));
        let stamp = stamp.or_else(|| modified(&entry)).unwrap_or(now);
        if now.saturating_sub(stamp) <= KEEP_SECONDS {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => swept.removed += 1,
            Err(error) => {
                swept.failed += 1;
                tracing::warn!("cannot remove expired attachment {name}: {error}");
            }
        }
    }
    swept
}

/// A store file (`<id>.<ext>` or `<id>.json` for a valid id) or one of its temp files.
fn is_ours(name: &str) -> bool {
    if name.starts_with(TEMP_PREFIX) {
        return true;
    }
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
