//! Host-job state under `<state root>/verification/<stage>/`: the stage's
//! current record, its lock, and one directory per job holding the record's
//! copy, progress, result, logs and the criterion process groups.
//!
//! `work_dir` may itself be a symlink (a worktree's `.loom/work`), so it is
//! canonicalized once per call; everything beneath it is opened through
//! `fs::safe_fs` with no symlink followed at any component (`store_fs`). A
//! symlinked `verification`, stage directory, job directory or target file is
//! refused before anything is created or locked. Every write is a temp file
//! renamed into place in the same directory.

use std::borrow::Cow;
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use nix::fcntl::AtFlags;
use nix::sys::stat::fstatat;

use super::store_fs::{
    append_locked, create_dirs, ends_with, open_root, read_in, read_json, refuse_unsafe,
    remove_job_dir, write_atomic, FILE_MODE,
};
use super::{CriterionOutcome, JobRecord, JobResult, JobState};
use crate::fs::safe_fs::{self, MAX_LOG_BYTES};
use crate::fs::safe_read::{is_not_found, list_dir_no_follow, EntryKind};

pub const VERIFICATION_DIR: &str = "verification";
const CURRENT_FILE: &str = "current.json";
const LOCK_FILE: &str = ".lock";
const RECORD_FILE: &str = "record.json";
const PROGRESS_FILE: &str = "progress.jsonl";
const RESULT_FILE: &str = "result.json";
const GROUPS_FILE: &str = "groups";
const CHILD_LOG_FILE: &str = "child.log";
/// `progress.jsonl` takes no outcome past this size but the truncation marker.
const PROGRESS_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Lines `groups` holds at most; a pgid is at most ten digits and a newline.
const MAX_GROUPS: usize = 4_096;
const GROUPS_MAX_BYTES: usize = MAX_GROUPS * 11;
/// Job directories a stage keeps; older ones go when a job is queued.
const KEPT_JOB_DIRS: usize = 3;
const RECORD_MAX_BYTES: usize = 1024 * 1024;
const TRUNCATION_LINE: &[u8] = b"\n[truncated at 4 MiB]\n";

/// The flocked `verification/<stage>/.lock`; dropping it unlocks.
pub struct StageLock {
    _fd: OwnedFd,
}

/// `<work_dir>/verification/<stage_id>`, for a valid stage id.
pub fn stage_dir(work_dir: &Path, stage_id: &str) -> Result<PathBuf> {
    Ok(work_dir.join(stage_rel(stage_id)?))
}

/// `<work_dir>/verification/<stage_id>/<job_id>`, for valid ids.
pub fn job_dir(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<PathBuf> {
    Ok(work_dir.join(job_rel(stage_id, job_id)?))
}

/// Take the stage's lock, creating its directory. Read-modify-write of
/// `current.json` happens only under it.
pub fn lock_stage(work_dir: &Path, stage_id: &str) -> Result<StageLock> {
    let rel = stage_rel(stage_id)?;
    let lock = rel.join(LOCK_FILE);
    let root = open_root(work_dir)?;
    refuse_unsafe(&root, &lock)?;
    create_dirs(&root, &rel)?;
    let flags = libc::O_RDWR | libc::O_CREAT;
    let fd = safe_fs::open_safely(root.as_raw_fd(), &lock, flags, FILE_MODE)
        .with_context(|| format!("cannot open the lock of stage '{stage_id}'"))?;
    safe_fs::flock_exclusive(&fd)?;
    Ok(StageLock { _fd: fd })
}

pub fn load_current(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>> {
    read_json(
        work_dir,
        &stage_rel(stage_id)?.join(CURRENT_FILE),
        RECORD_MAX_BYTES,
    )
}

/// Write `record` as its stage's current record and as `<job dir>/record.json`,
/// creating the job directory.
pub fn save_current(work_dir: &Path, record: &JobRecord) -> Result<()> {
    let job = job_rel(&record.stage_id, &record.id)?;
    let current = stage_rel(&record.stage_id)?.join(CURRENT_FILE);
    let copy = job.join(RECORD_FILE);
    let root = open_root(work_dir)?;
    refuse_unsafe(&root, &current)?;
    refuse_unsafe(&root, &copy)?;
    create_dirs(&root, &job)?;
    let bytes = serde_json::to_vec_pretty(record).context("cannot encode the job record")?;
    write_atomic(&root, &copy, &bytes)?;
    write_atomic(&root, &current, &bytes)
}

pub fn load_job_record(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Option<JobRecord>> {
    let rel = job_rel(stage_id, job_id)?.join(RECORD_FILE);
    read_json(work_dir, &rel, RECORD_MAX_BYTES)
}

/// Append one outcome line to `progress.jsonl`. Past 2 MiB the file takes
/// one final `progress-truncated` outcome and ignores every later append.
pub fn append_progress(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    outcome: &CriterionOutcome,
) -> Result<()> {
    let rel = job_rel(stage_id, job_id)?.join(PROGRESS_FILE);
    let line = json_line(outcome)?;
    let marker = json_line(&CriterionOutcome {
        label: "progress-truncated".to_string(),
        passed: false,
        summary: "progress truncated at 2 MiB".to_string(),
        log: None,
    })?;
    append_locked(&open_root(work_dir)?, &rel, |file, len| {
        if ends_with(file, len, &marker)? {
            Ok(None)
        } else if len.saturating_add(line.len() as u64) <= PROGRESS_MAX_BYTES {
            Ok(Some(line))
        } else {
            Ok(Some(marker))
        }
    })
}

pub fn read_progress(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
) -> Result<Vec<CriterionOutcome>> {
    let rel = job_rel(stage_id, job_id)?.join(PROGRESS_FILE);
    let max = PROGRESS_MAX_BYTES as usize + RECORD_MAX_BYTES;
    let Some(bytes) = read_in(work_dir, &rel, max)? else {
        return Ok(Vec::new());
    };
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).context("malformed progress line"))
        .collect()
}

pub fn save_result(work_dir: &Path, stage_id: &str, result: &JobResult) -> Result<()> {
    let rel = job_rel(stage_id, &result.job_id)?.join(RESULT_FILE);
    let bytes = serde_json::to_vec_pretty(result).context("cannot encode the job result")?;
    write_atomic(&open_root(work_dir)?, &rel, &bytes)
}

pub fn load_result(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Option<JobResult>> {
    let rel = job_rel(stage_id, job_id)?.join(RESULT_FILE);
    read_json(work_dir, &rel, MAX_LOG_BYTES)
}

/// Write a criterion's full output to `<job dir>/<label>.log` and return the
/// file name. Output past `MAX_LOG_BYTES` is cut so that it and the final
/// `[truncated at 4 MiB]` line fit within it.
pub fn save_criterion_log(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    label: &str,
    bytes: &[u8],
) -> Result<String> {
    let valid = |byte: u8| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b':' | b'_' | b'-');
    ensure!(
        !label.is_empty() && label.bytes().all(valid),
        "criterion label {label:?} is not [a-z0-9:_-]+"
    );
    let name = format!("{label}.log");
    let rel = job_rel(stage_id, job_id)?.join(&name);
    write_atomic(&open_root(work_dir)?, &rel, &capped_log(bytes))?;
    Ok(name)
}

/// Write `<job dir>/<name>` atomically; `name` is `[a-z0-9._-]+` and does not
/// start with a dot (the lock and the temp files do).
pub fn save_job_file(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    name: &str,
    bytes: &[u8],
) -> Result<()> {
    let rel = job_file_rel(stage_id, job_id, name)?;
    ensure!(
        bytes.len() <= MAX_LOG_BYTES,
        "job file {name} exceeds {MAX_LOG_BYTES} bytes"
    );
    write_atomic(&open_root(work_dir)?, &rel, bytes)
}

pub fn load_job_file(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    name: &str,
) -> Result<Option<Vec<u8>>> {
    read_in(
        work_dir,
        &job_file_rel(stage_id, job_id, name)?,
        MAX_LOG_BYTES,
    )
}

/// Record a criterion's process group; `groups` takes at most 4,096 lines.
pub fn append_group(work_dir: &Path, stage_id: &str, job_id: &str, pgid: u32) -> Result<()> {
    let rel = job_rel(stage_id, job_id)?.join(GROUPS_FILE);
    append_locked(&open_root(work_dir)?, &rel, |file, _| {
        let mut held = Vec::new();
        file.take(GROUPS_MAX_BYTES as u64)
            .read_to_end(&mut held)
            .context("cannot read the job's process groups")?;
        let lines = held.iter().filter(|byte| **byte == b'\n').count();
        Ok((lines < MAX_GROUPS).then(|| format!("{pgid}\n").into_bytes()))
    })
}

/// The process groups the job's child recorded; a missing file is none. A
/// value of 0 or 1 is dropped: signalling it would reach far beyond the job.
pub fn load_groups(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Vec<u32>> {
    let rel = job_rel(stage_id, job_id)?.join(GROUPS_FILE);
    let Some(bytes) = read_in(work_dir, &rel, GROUPS_MAX_BYTES)? else {
        return Ok(Vec::new());
    };
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pgid| *pgid > 1)
        .collect())
}

/// Open `<job dir>/child.log` for appending (and reading its tail), creating it.
pub(crate) fn open_child_log(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<File> {
    let rel = job_rel(stage_id, job_id)?.join(CHILD_LOG_FILE);
    let root = open_root(work_dir)?;
    let flags = libc::O_RDWR | libc::O_APPEND | libc::O_CREAT;
    let fd = safe_fs::open_safely(root.as_raw_fd(), &rel, flags, FILE_MODE)
        .with_context(|| format!("cannot open {}", rel.display()))?;
    Ok(File::from(fd))
}

/// Every stage with a directory under `verification/`, by valid id.
pub(crate) fn stage_ids(work_dir: &Path) -> Result<Vec<String>> {
    let root = match open_root(work_dir) {
        Err(error) if is_not_found(&error) => return Ok(Vec::new()),
        root => root?,
    };
    match list_dir_no_follow(&root, Path::new(VERIFICATION_DIR)) {
        Ok(entries) => Ok(entries
            .into_iter()
            .filter(|(_, kind)| *kind == EntryKind::Directory)
            .filter_map(|(name, _)| name.into_string().ok())
            .filter(|name| crate::validation::validate_id(name).is_ok())
            .collect()),
        Err(error) if is_not_found(&error) => Ok(Vec::new()),
        Err(error) => Err(error.context("cannot list the verification directory")),
    }
}

/// Remove the stage's job directories but the three newest by mtime, and
/// never `keep`'s.
pub(crate) fn prune_jobs(work_dir: &Path, stage_id: &str, keep: &str) -> Result<()> {
    let rel = stage_rel(stage_id)?;
    let root = open_root(work_dir)?;
    let flags = libc::O_DIRECTORY | libc::O_RDONLY;
    let stage = safe_fs::open_safely(root.as_raw_fd(), &rel, flags, 0)?;
    let mut jobs = Vec::new();
    for (name, kind) in list_dir_no_follow(&stage, Path::new(""))? {
        let Some(name) = name.to_str().filter(|name| validate_job_id(name).is_ok()) else {
            continue;
        };
        if kind == EntryKind::Directory {
            let stat = fstatat(&stage, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
            jobs.push(((stat.st_mtime, stat.st_mtime_nsec), name.to_string()));
        }
    }
    jobs.sort_unstable_by(|a, b| b.cmp(a));
    for (_, name) in jobs.into_iter().skip(KEPT_JOB_DIRS) {
        if name != keep && !awaiting_retire(work_dir, stage_id, &name) {
            remove_job_dir(&stage, &name)
                .with_context(|| format!("cannot prune job directory {name}"))?;
        }
    }
    Ok(())
}

/// How long after `finished_at` a cancelled job that ran may still be held by
/// the runner; the runner reaps it on its next tick, well inside this.
const RETIRE_GRACE_SECS: i64 = 60;

/// Whether the runner may still need the job directory: a non-terminal job,
/// or a cancelled one that ran (`pid` set) and ended within the grace. The
/// runner's `retire` reads the job's `groups` file to kill its criteria, so
/// pruning the directory first would orphan them. An unreadable record is
/// not protected, so a corrupt directory cannot pin itself forever.
fn awaiting_retire(work_dir: &Path, stage_id: &str, job_id: &str) -> bool {
    let Ok(Some(record)) = load_job_record(work_dir, stage_id, job_id) else {
        return false;
    };
    if !record.state.is_terminal() {
        return true;
    }
    let recent = record
        .finished_at
        .is_none_or(|at| (chrono::Utc::now() - at).num_seconds() < RETIRE_GRACE_SECS);
    record.state == JobState::Cancelled && record.pid.is_some() && recent
}

/// A job id is exactly 32 lowercase hex digits.
pub(crate) fn validate_job_id(job_id: &str) -> Result<()> {
    let hex = |byte: u8| matches!(byte, b'0'..=b'9' | b'a'..=b'f');
    if job_id.len() == 32 && job_id.bytes().all(hex) {
        return Ok(());
    }
    bail!("job id {job_id:?} is not 32 lowercase hex digits")
}

fn stage_rel(stage_id: &str) -> Result<PathBuf> {
    crate::validation::validate_id(stage_id)?;
    Ok(Path::new(VERIFICATION_DIR).join(stage_id))
}

fn job_rel(stage_id: &str, job_id: &str) -> Result<PathBuf> {
    validate_job_id(job_id)?;
    Ok(stage_rel(stage_id)?.join(job_id))
}

fn job_file_rel(stage_id: &str, job_id: &str, name: &str) -> Result<PathBuf> {
    let valid = |byte: u8| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-');
    ensure!(
        !name.is_empty() && !name.starts_with('.') && name.bytes().all(valid),
        "job file name {name:?} is not [a-z0-9._-]+ or starts with a dot"
    );
    Ok(job_rel(stage_id, job_id)?.join(name))
}

fn json_line(outcome: &CriterionOutcome) -> Result<Vec<u8>> {
    let mut line = serde_json::to_vec(outcome).context("cannot encode the outcome")?;
    line.push(b'\n');
    Ok(line)
}

fn capped_log(bytes: &[u8]) -> Cow<'_, [u8]> {
    if bytes.len() <= MAX_LOG_BYTES {
        return Cow::Borrowed(bytes);
    }
    let mut capped = bytes[..MAX_LOG_BYTES - TRUNCATION_LINE.len()].to_vec();
    capped.extend_from_slice(TRUNCATION_LINE);
    Cow::Owned(capped)
}
