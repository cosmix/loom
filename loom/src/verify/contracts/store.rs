//! `.loom/work/contracts/<stage>/`: the freeze record, the frozen copies and
//! the contract session's respawn budget (DESIGN D8).
//!
//! | Path | Written by |
//! | --- | --- |
//! | `freeze.json` | the daemon's freeze handler, once; never replaced |
//! | `files/<path>` | the same handler, before `freeze.json` |
//! | `attempts` | the daemon, when it hands out a contract session |
//!
//! `freeze.json` is written last and atomically: its presence is what ends the
//! contract phase, so it must never describe copies that are not on disk yet.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::os::fd::AsRawFd;
use std::path::{Component, Path, PathBuf};

use crate::daemon::ContractRunReport;
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_locked_write_in_workdir, safe_open_dirfd,
};
use crate::fs::safe_read::{is_not_found, read_bounded};

pub const FREEZE_RECORD_VERSION: u32 = 1;
/// The largest file a freeze copies, and so the largest a completion re-reads.
pub const MAX_FROZEN_FILE_BYTES: usize = crate::fs::safe_fs::MAX_LOG_BYTES;
const CONTRACTS_DIR: &str = "contracts";
const FREEZE_FILE: &str = "freeze.json";
const FILES_DIR: &str = "files";
const ATTEMPTS_FILE: &str = "attempts";
const MAX_FREEZE_BYTES: usize = 1024 * 1024;
const MAX_ATTEMPTS_BYTES: usize = 64;

/// `freeze.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreezeRecord {
    pub version: u32,
    pub stage_id: String,
    pub session_id: String,
    pub frozen_at: DateTime<Utc>,
    /// The commit the stage branch forked from.
    pub base: String,
    pub files: Vec<FrozenFile>,
    pub contracts: Vec<FrozenContract>,
}

/// One frozen file; `path` is relative to the stage's working directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenFile {
    pub path: String,
    pub sha256: String,
}

/// How one contract failed when it was frozen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenContract {
    pub id: String,
    pub adapter: Option<String>,
    pub outcome: String,
    pub exit_code: Option<i32>,
}

impl From<&ContractRunReport> for FrozenContract {
    fn from(report: &ContractRunReport) -> Self {
        Self {
            id: report.contract_id.clone(),
            adapter: report.adapter.clone(),
            outcome: report.outcome.clone(),
            exit_code: report.exit_code,
        }
    }
}

/// Refuse a path that is empty, absolute, or has any component other than a
/// plain name: a frozen path must stay beneath the directory it is joined to.
pub fn validate_relative(path: &str) -> Result<()> {
    let mut components = Path::new(path).components().peekable();
    let plain = components.peek().is_some()
        && components.all(|component| matches!(component, Component::Normal(_)));
    if !plain {
        bail!("'{path}' is not a plain relative path");
    }
    Ok(())
}

/// Where the frozen copy of `rel` lives. `rel` must already have passed
/// [`validate_relative`]; every path in a record [`load_freeze`] returns has.
pub fn frozen_file_path(work_dir: &Path, stage_id: &str, rel: &str) -> PathBuf {
    stage_dir(work_dir, stage_id).join(FILES_DIR).join(rel)
}

/// The stage's freeze record, or `None` before the contract phase froze.
pub fn load_freeze(work_dir: &Path, stage_id: &str) -> Result<Option<FreezeRecord>> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let relative = Path::new(CONTRACTS_DIR).join(stage_id).join(FREEZE_FILE);
    let bytes = match read_bounded(&root, &relative, MAX_FREEZE_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let record: FreezeRecord = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid freeze record for stage '{stage_id}'"))?;
    if record.stage_id != stage_id {
        bail!(
            "freeze record for stage '{stage_id}' names stage '{}'",
            record.stage_id
        );
    }
    for file in &record.files {
        validate_relative(&file.path)
            .with_context(|| format!("freeze record for stage '{stage_id}'"))?;
    }
    Ok(Some(record))
}

/// Copy `files` (path relative to the working directory, content) under
/// `files/`, then write `record` as `freeze.json`. Refuses when a freeze
/// record already exists: a freeze is never replaced.
pub fn write_freeze(
    work_dir: &Path,
    record: &FreezeRecord,
    files: &[(String, Vec<u8>)],
) -> Result<()> {
    let root = canonical_work_dir(work_dir, &record.stage_id)?;
    for (path, _) in files {
        validate_relative(path)?;
    }
    let dir = stage_dir(&root, &record.stage_id);
    let json = serde_json::to_string_pretty(record)?;
    locked_dir_update(&dir, || {
        let freeze_path = dir.join(FREEZE_FILE);
        if freeze_path.exists() {
            bail!(
                "contracts of stage '{}' are already frozen",
                record.stage_id
            );
        }
        copy_files(&dir, files)?;
        atomic_write_locked(&freeze_path, &json)
    })
}

fn copy_files(dir: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    let dirfd = safe_open_dirfd(dir)?;
    for (path, bytes) in files {
        let target = Path::new(FILES_DIR).join(path);
        if let Some(parent) = target.parent() {
            safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), parent, 0o755)?;
        }
        safe_locked_write_in_workdir(dirfd.as_raw_fd(), &target, bytes)
            .with_context(|| format!("failed to copy frozen file '{path}'"))?;
    }
    Ok(())
}

/// Contract sessions handed out for the stage so far.
pub fn attempts_spent(work_dir: &Path, stage_id: &str) -> Result<u32> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    read_attempts(&root, stage_id)
}

/// Spend one attempt and return the new total. Called when a contract session
/// is handed out, never derived from what that session later produces.
pub fn spend_attempt(work_dir: &Path, stage_id: &str) -> Result<u32> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let dir = stage_dir(&root, stage_id);
    locked_dir_update(&dir, || {
        let spent = read_attempts(&root, stage_id)?.saturating_add(1);
        atomic_write_locked(&dir.join(ATTEMPTS_FILE), &format!("{spent}\n"))?;
        Ok(spent)
    })
}

fn read_attempts(root: &Path, stage_id: &str) -> Result<u32> {
    let relative = Path::new(CONTRACTS_DIR).join(stage_id).join(ATTEMPTS_FILE);
    let bytes = match read_bounded(root, &relative, MAX_ATTEMPTS_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(0),
        Err(error) => return Err(error),
    };
    String::from_utf8_lossy(&bytes)
        .trim()
        .parse()
        .with_context(|| format!("invalid contract attempts count for stage '{stage_id}'"))
}

fn stage_dir(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir.join(CONTRACTS_DIR).join(stage_id)
}

/// Validate `stage_id` before it becomes a path, and resolve a worktree's
/// `.loom/work` symlink so every dirfd-anchored access starts at the real
/// directory. The review store resolves its paths the same way.
pub(in crate::verify) fn canonical_work_dir(work_dir: &Path, stage_id: &str) -> Result<PathBuf> {
    crate::validation::validate_id(stage_id).context("invalid stage id")?;
    work_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", work_dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn record(stage_id: &str, path: &str, sha256: &str) -> FreezeRecord {
        FreezeRecord {
            version: FREEZE_RECORD_VERSION,
            stage_id: stage_id.to_string(),
            session_id: "session-1".to_string(),
            frozen_at: Utc::now(),
            base: "a".repeat(40),
            files: vec![FrozenFile {
                path: path.to_string(),
                sha256: sha256.to_string(),
            }],
            contracts: Vec::new(),
        }
    }

    #[test]
    fn write_then_load_round_trips_and_refuses_a_second_freeze() {
        let tmp = TempDir::new().unwrap();
        let bytes = b"fn contract() {}\n".to_vec();
        let frozen = record("s1", "tests/a_contract.rs", "abc");
        let files = vec![("tests/a_contract.rs".to_string(), bytes.clone())];

        write_freeze(tmp.path(), &frozen, &files).unwrap();

        assert_eq!(load_freeze(tmp.path(), "s1").unwrap(), Some(frozen.clone()));
        let copy = frozen_file_path(tmp.path(), "s1", "tests/a_contract.rs");
        assert_eq!(std::fs::read(copy).unwrap(), bytes);
        let again = write_freeze(tmp.path(), &frozen, &files).unwrap_err();
        assert!(again.to_string().contains("already frozen"), "{again:#}");
    }

    #[test]
    fn a_stage_without_a_freeze_loads_none() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(load_freeze(tmp.path(), "s1").unwrap(), None);
    }

    #[test]
    fn escaping_paths_and_ids_are_refused() {
        let tmp = TempDir::new().unwrap();
        for path in ["../x.rs", "/etc/passwd", "a/../../x", "./a.rs", ""] {
            assert!(validate_relative(path).is_err(), "{path}");
            let files = vec![(path.to_string(), Vec::new())];
            assert!(write_freeze(tmp.path(), &record("s1", "a.rs", "x"), &files).is_err());
        }
        assert!(load_freeze(tmp.path(), "../escape").is_err());
        assert!(spend_attempt(tmp.path(), "../escape").is_err());
    }

    #[test]
    fn attempts_are_counted_as_they_are_spent() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(attempts_spent(tmp.path(), "s1").unwrap(), 0);
        assert_eq!(spend_attempt(tmp.path(), "s1").unwrap(), 1);
        assert_eq!(spend_attempt(tmp.path(), "s1").unwrap(), 2);
        assert_eq!(attempts_spent(tmp.path(), "s1").unwrap(), 2);
    }
}
