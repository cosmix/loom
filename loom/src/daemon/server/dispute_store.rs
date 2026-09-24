//! The filing steps every dispute shares, whatever it contests: the per-stage
//! lock, the next sequential id, the create-new `request.md`, and the
//! escalation of an exhausted budget. `dispute.rs` states the trust boundary.

use anyhow::{anyhow, bail, Result};
use std::fs::File;
use std::os::fd::OwnedFd;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs::safe_create_new_in_workdir;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::DisputeRequest;
use crate::models::stage::Stage;
use crate::verify::transitions::update_stage;

/// A stage's `disputes/<stage>/` directory, held under its per-stage flock
/// until this is dropped.
pub(super) struct LockedDisputes {
    /// The canonical state directory.
    pub work_dir: PathBuf,
    /// `disputes/<stage>/` beneath it.
    pub stage_dir: PathBuf,
    _lock: File,
}

/// Take the lock that serialises dispute filings for `stage_id` (id
/// allocation and state transition). `stage_id` must already be validated.
pub(super) fn lock_stage_disputes(work_dir: &Path, stage_id: &str) -> Result<LockedDisputes> {
    // Resolve canonical .loom/work path. Worktrees use a `.loom/work` symlink
    // to ../../../.loom/work; canonicalize so the dirfd-relative writes land
    // in the real directory and so the per-stage lock paths align.
    let work_canonical = work_dir.canonicalize().map_err(|e| {
        anyhow!(
            "Failed to canonicalize work_dir {}: {e}",
            work_dir.display()
        )
    })?;

    let wd = WorkDir::new(&work_canonical).map_err(|e| {
        anyhow!(
            "Failed to load WorkDir at {}: {e}",
            work_canonical.display()
        )
    })?;
    // Note: WorkDir::new may search upward — for an already-canonical
    // state-root path it returns that path. Use the canonical work path for
    // disputes_dir() so all writes land beneath it deterministically.
    let stage_dir = wd.disputes_dir().join(stage_id);
    std::fs::create_dir_all(&stage_dir)?;

    let lock_path = stage_dir.join(".lock");
    let lock_file: File = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    // SAFETY: the descriptor belongs to the live `lock_file`, and `LOCK_EX` is
    // a valid flock operation for the duration of this call.
    let rc = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        bail!("Failed to acquire dispute lock at {}", lock_path.display());
    }
    // The lock is released when `_lock` closes on drop.
    Ok(LockedDisputes {
        work_dir: work_canonical,
        stage_dir,
        _lock: lock_file,
    })
}

/// Write `record` as `<n>/request.md` under the next free id, created new, and
/// return that id (it replaces `record.id`).
pub(super) fn write_request(stage_dir: &Path, mut record: DisputeRequest) -> Result<u32> {
    // Retry id allocation on EEXIST up to 3 times to handle the rare case
    // where a concurrent caller (under a different lock domain) snuck a dir
    // in between our enumeration and create.
    let mut id = next_dispute_id(stage_dir)?;
    let mut attempts = 0;
    let dispute_dir = loop {
        let dir = stage_dir.join(id.to_string());
        match std::fs::create_dir(&dir) {
            Ok(_) => break dir,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempts < 3 => {
                attempts += 1;
                id = next_dispute_id(stage_dir)?;
            }
            Err(e) => bail!("Failed to create dispute directory {}: {e}", dir.display()),
        }
    };

    record.id = id;
    let yaml = serde_yaml::to_string(&record)?;
    let content = format!(
        "---\n{yaml}---\n\n# Dispute request {id} for stage {}\n",
        record.stage_id
    );
    let dirfd = open_dir_fd(&dispute_dir)?;
    safe_create_new_in_workdir(
        dirfd.as_raw_fd(),
        Path::new("request.md"),
        content.as_bytes(),
    )?;
    Ok(id)
}

/// Escalate the stage to NeedsHumanReview because a dispute budget is
/// exhausted, so the agent does not loop futilely retrying the same failure.
/// `why` words the review reason from the fresh on-disk stage.
///
/// Re-read under the stages-dir lock and mutate only the review/status fields
/// this operation owns, so a concurrent orchestrator/CLI write to other fields
/// is preserved (A-5). A stage the state machine will not escalate is logged
/// and left for an operator; the caller still refuses the dispute.
pub(super) fn escalate_to_human_review(
    stage_id: &str,
    work_dir: &Path,
    why: impl FnOnce(&Stage) -> String,
) {
    let escalate = update_stage(stage_id, work_dir, |s| {
        let reason = why(s);
        s.try_request_human_review(reason)
    });
    if let Err(e) = escalate {
        tracing::warn!(
            target: "loom::dispute",
            stage = %stage_id,
            error = %e,
            "dispute budget exhausted but stage could not be escalated to NeedsHumanReview",
        );
    }
}

/// `max(existing numeric entry) + 1` in `stage_dir`, starting at 1.
fn next_dispute_id(stage_dir: &Path) -> Result<u32> {
    let mut max_id: u32 = 0;
    if !stage_dir.exists() {
        return Ok(1);
    }
    for entry in std::fs::read_dir(stage_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = match name.to_str() {
            Some(n) => n,
            None => continue,
        };
        if let Ok(id) = name.parse::<u32>() {
            if id > max_id {
                max_id = id;
            }
        }
    }
    Ok(max_id + 1)
}

fn open_dir_fd(path: &Path) -> Result<OwnedFd> {
    use std::os::fd::FromRawFd;
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())?;
    // SAFETY: `c_path` is NUL-terminated and the flags request a read-only
    // directory descriptor without transferring any Rust-owned pointer.
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_DIRECTORY | libc::O_RDONLY) };
    if fd < 0 {
        bail!(
            "Failed to open dispute directory {} for dirfd: {}",
            path.display(),
            std::io::Error::last_os_error()
        );
    }
    // SAFETY: a non-negative `open` result is a fresh descriptor whose
    // ownership is transferred exactly once to `OwnedFd`.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
