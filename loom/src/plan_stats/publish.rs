//! The archive: one file per run under `<loom home>/stats/plans/`, written once.
//!
//! The home copy outlives `loom clean`. A name is published by hard-linking a synced
//! temporary file, so the first record of a run stays and a reader never sees a partial one.

use std::fs::File;
use std::os::unix::io::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::record::{PlanStatsRecord, MAX_RECORD_BYTES};
use crate::fs::cache_io;
use crate::fs::safe_fs::{open_safely, safe_create_dir_all_in_workdir};
use crate::fs::safe_read::{is_not_found, read_bounded_at};
use crate::fs::safe_replace::create_new::publish_new_in_dir;

/// Directory of the archive below the loom home.
const ARCHIVE_DIR: &str = "stats/plans";
/// Longest archive file stem.
const MAX_STEM_LEN: usize = 128;
/// Archive files are private to the user.
const ARCHIVE_FILE_MODE: u32 = 0o600;
const ARCHIVE_DIR_MODE: libc::mode_t = 0o700;

/// What [`publish`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Published {
    /// This call wrote the archive file.
    Written(PathBuf),
    /// The run already had its archive file; it was left as it was.
    AlreadyPublished(PathBuf),
}

/// The loom user directory: the parent of the user config file, so `LOOM_HOME` and the
/// test redirect move the archive with it.
pub fn loom_home() -> Result<PathBuf> {
    crate::user_config::loom_user_dir()
}

/// `<plan_id>.json`, the plan id cut to a safe file stem: `[A-Za-z0-9_-]`, at most
/// [`MAX_STEM_LEN`] characters. A real plan id is already that and passes unchanged.
pub fn file_name(record: &PlanStatsRecord) -> String {
    file_name_for(&record.plan_id)
}

pub(super) fn file_name_for(plan_id: &str) -> String {
    let stem: String = plan_id
        .chars()
        .take(MAX_STEM_LEN)
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let stem = if stem.is_empty() { "plan" } else { &stem };
    format!("{stem}.json")
}

fn archive_dir(home: &Path) -> PathBuf {
    home.join(ARCHIVE_DIR)
}

/// Where the archive keeps the run `plan_id` below `home`.
pub fn archive_path(home: &Path, plan_id: &str) -> PathBuf {
    archive_dir(home).join(file_name_for(plan_id))
}

/// Archive `record` under `home` unless its run already has a file. An existing file must
/// be a complete record of the same run; anything else is an error and is not touched.
pub fn publish(record: &PlanStatsRecord, home: &Path) -> Result<Published> {
    let bytes = record.to_json()?;
    if bytes.len() > MAX_RECORD_BYTES {
        bail!(
            "the plan stats record is {} bytes, over the {MAX_RECORD_BYTES} limit",
            bytes.len()
        );
    }
    let dir = open_archive_dir(home)?;
    let name = file_name(record);
    let path = archive_dir(home).join(&name);
    let written = publish_new_in_dir(&dir, name.as_ref(), &bytes, ARCHIVE_FILE_MODE)
        .with_context(|| format!("cannot write {}", path.display()))?;
    if written {
        return Ok(Published::Written(path));
    }
    match read_in(&dir, &name)? {
        Some(existing) if existing.plan_id == record.plan_id => {
            Ok(Published::AlreadyPublished(path))
        }
        Some(existing) => bail!(
            "{} holds the record of another run ({})",
            path.display(),
            existing.plan_id
        ),
        None => bail!("{} vanished while it was being published", path.display()),
    }
}

/// The archived record of `plan_id`, `None` when the run has none yet. A file that is
/// unreadable, over the bound, not a record, or another run's is an error.
pub(super) fn archived(home: &Path, plan_id: &str) -> Result<Option<PlanStatsRecord>> {
    let Some(dir) = open_existing_archive_dir(home)? else {
        return Ok(None);
    };
    let record = read_in(&dir, &file_name_for(plan_id))?;
    if let Some(record) = &record {
        if record.plan_id != plan_id {
            bail!(
                "{} holds the record of another run ({})",
                archive_path(home, plan_id).display(),
                record.plan_id
            );
        }
    }
    Ok(record)
}

/// The record in `name`, `None` when there is no such entry.
fn read_in(dir: &OwnedFd, name: &str) -> Result<Option<PlanStatsRecord>> {
    let bytes = match read_bounded_at(dir, Path::new(name), MAX_RECORD_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error.context(format!("cannot read the archived {name}"))),
    };
    PlanStatsRecord::from_json(&bytes)
        .map(Some)
        .with_context(|| format!("the archived {name} is not a plan stats record"))
}

/// The home directory itself may be a link (a dotfiles setup); everything below it is
/// opened without following one.
fn open_home(home: &Path) -> Result<OwnedFd> {
    let dir = File::open(home).with_context(|| format!("cannot open {}", home.display()))?;
    if !dir.metadata()?.is_dir() {
        bail!("{} is not a directory", home.display());
    }
    Ok(dir.into())
}

fn open_archive_dir(home: &Path) -> Result<OwnedFd> {
    cache_io::create_dir(home)?;
    let home_fd = open_home(home)?;
    safe_create_dir_all_in_workdir(
        home_fd.as_raw_fd(),
        Path::new(ARCHIVE_DIR),
        ARCHIVE_DIR_MODE,
    )
    .with_context(|| format!("cannot create {}", archive_dir(home).display()))?;
    open_dir_below(&home_fd, home)
}

/// The archive directory, `None` when the home or the directory does not exist yet.
fn open_existing_archive_dir(home: &Path) -> Result<Option<OwnedFd>> {
    let home_fd = match open_home(home) {
        Ok(fd) => fd,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    match open_dir_below(&home_fd, home) {
        Ok(fd) => Ok(Some(fd)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn open_dir_below(home_fd: &OwnedFd, home: &Path) -> Result<OwnedFd> {
    let flags = libc::O_DIRECTORY | libc::O_RDONLY;
    open_safely(home_fd.as_raw_fd(), Path::new(ARCHIVE_DIR), flags, 0)
        .with_context(|| format!("refusing to open {}", archive_dir(home).display()))
}
