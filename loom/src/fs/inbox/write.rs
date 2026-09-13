//! Writing one relayed request into `W/inbox/<session-id>/<id>.json`.
//!
//! Create-in-`.tmp`-then-`link()` so the daemon's drain never observes a
//! partially written entry: `link()` only succeeds once the tmp file's full
//! content already landed and was fsynced
//! (`doc/plans/PLAN-loom-state-confinement.md` section 7, steps 9-10).

use std::ffi::CString;
use std::io;
use std::os::unix::io::{AsRawFd, OwnedFd, RawFd};
use std::path::Path;

use anyhow::{Context, Result};

use super::ledger::is_recorded;
use super::paths::{
    entry_file_name, inbox_root_relpath, session_inbox, session_relpath, tmp_dir_relpath,
};
use crate::fs::safe_fs::{
    open_safely, safe_create_dir_all_in_workdir, safe_create_new_in_workdir, safe_open_dirfd,
};
use crate::relay::{InboxEntry, MAX_PENDING_ENTRIES};

/// Outcome of [`write_entry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The entry landed at `<id>.json`.
    Written,
    /// The ledger already recorded this id, or another writer's `link()` won
    /// the race — either way the request was already relayed.
    AlreadyRelayed,
    /// The session already has `MAX_PENDING_ENTRIES` undrained entries.
    Capacity,
}

/// Relay `entry` into its session's inbox.
///
/// `entry.session_id` names the destination directory directly — there is no
/// separate "target" session id it could disagree with, so the invariant
/// that an entry only ever lands under its own session holds by
/// construction. An invalid session id or request id is refused, not routed
/// elsewhere.
pub fn write_entry(work_dir: &Path, entry: &InboxEntry) -> Result<WriteOutcome> {
    if is_recorded(work_dir, &entry.session_id, &entry.id)? {
        return Ok(WriteOutcome::AlreadyRelayed);
    }

    let dirfd = safe_open_dirfd(work_dir)?;
    let raw = dirfd.as_raw_fd();
    let session_rel = session_relpath(&entry.session_id)?;
    let tmp_rel = tmp_dir_relpath(&entry.session_id)?;
    safe_create_dir_all_in_workdir(raw, &inbox_root_relpath(), 0o700)?;
    safe_create_dir_all_in_workdir(raw, &session_rel, 0o700)?;
    safe_create_dir_all_in_workdir(raw, &tmp_rel, 0o700)?;

    let session_dir = session_inbox(work_dir, &entry.session_id)?;
    if count_json_entries(&session_dir)? >= MAX_PENDING_ENTRIES {
        return Ok(WriteOutcome::Capacity);
    }

    let tmp_name = format!("{}.{}", entry.id, std::process::id());
    let final_name = entry_file_name(&entry.id)?;
    let tmp_entry_rel = tmp_rel.join(&tmp_name);

    safe_create_new_in_workdir(raw, &tmp_entry_rel, &entry.encode())?;
    fsync_relpath(raw, &tmp_entry_rel)?;

    let tmp_dirfd = open_safely(raw, &tmp_rel, libc::O_DIRECTORY | libc::O_RDONLY, 0)?;
    let session_dirfd = open_safely(raw, &session_rel, libc::O_DIRECTORY | libc::O_RDONLY, 0)?;
    let tmp_name_c = name_to_cstring(&tmp_name)?;
    let final_name_c = name_to_cstring(&final_name)?;

    let link_result = link_names(&tmp_dirfd, &tmp_name_c, &session_dirfd, &final_name_c);
    // The tmp file is scratch space either way: drop it once the link
    // attempt (successful or not) has settled.
    unlink_name(&tmp_dirfd, &tmp_name_c);

    match link_result {
        Ok(true) => {
            fsync_owned(&session_dirfd, &session_rel)?;
            Ok(WriteOutcome::Written)
        }
        Ok(false) => Ok(WriteOutcome::AlreadyRelayed),
        Err(error) => Err(error),
    }
}

fn count_json_entries(dir: &Path) -> Result<usize> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to list {}", dir.display()))
        }
    };
    let mut count = 0usize;
    for entry in read_dir {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        if entry.file_name().to_string_lossy().ends_with(".json") {
            count += 1;
        }
    }
    Ok(count)
}

fn name_to_cstring(name: &str) -> Result<CString> {
    CString::new(name.as_bytes().to_vec()).context("inbox: name contains an interior NUL byte")
}

/// `linkat(2)` of `old_name` under `old_dirfd` to `new_name` under
/// `new_dirfd`. `Ok(true)` on success, `Ok(false)` on `EEXIST` (another
/// writer already relayed this id), `Err` for anything else.
fn link_names(
    old_dirfd: &OwnedFd,
    old_name: &CString,
    new_dirfd: &OwnedFd,
    new_name: &CString,
) -> Result<bool> {
    // SAFETY: both dirfds were opened by `open_safely` and remain valid for
    // this call; both names are NUL-terminated single path components.
    let r = unsafe {
        libc::linkat(
            old_dirfd.as_raw_fd(),
            old_name.as_ptr(),
            new_dirfd.as_raw_fd(),
            new_name.as_ptr(),
            0,
        )
    };
    if r == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EEXIST) {
        return Ok(false);
    }
    Err(error).context("inbox: linkat failed")
}

/// Best-effort `unlinkat(2)`; a missing tmp file is not an error since the
/// caller reaches this after both a successful and a failed link attempt.
fn unlink_name(dirfd: &OwnedFd, name: &CString) {
    // SAFETY: dirfd is valid; name is NUL-terminated.
    unsafe {
        libc::unlinkat(dirfd.as_raw_fd(), name.as_ptr(), 0);
    }
}

fn fsync_owned(fd: &OwnedFd, relpath: &Path) -> Result<()> {
    // SAFETY: fd is valid for the duration of this call.
    if unsafe { libc::fsync(fd.as_raw_fd()) } < 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("inbox: fsync failed on {}", relpath.display()));
    }
    Ok(())
}

fn fsync_relpath(dirfd: RawFd, relpath: &Path) -> Result<()> {
    let fd = open_safely(dirfd, relpath, libc::O_RDONLY, 0)
        .with_context(|| format!("inbox: failed to reopen {} for fsync", relpath.display()))?;
    fsync_owned(&fd, relpath)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::{new_request_id, AgentRole, RequestKind};
    use chrono::Utc;
    use std::os::unix::fs::PermissionsExt;

    fn sample_entry(session_id: &str, id: &str) -> InboxEntry {
        InboxEntry {
            v: 1,
            id: id.to_string(),
            kind: RequestKind::Memory,
            relayed_at: Utc::now(),
            session_id: session_id.to_string(),
            stage_id: "stage-a".to_string(),
            agent: AgentRole::Main,
            tool_use_id: Some("tool-1".to_string()),
            payload: serde_json::json!({"content": "note"}),
        }
    }

    #[test]
    fn writes_a_new_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let entry = sample_entry("session-1", &id);

        assert_eq!(
            write_entry(tmp.path(), &entry).unwrap(),
            WriteOutcome::Written
        );

        let path = tmp
            .path()
            .join("inbox")
            .join("session-1")
            .join(format!("{id}.json"));
        assert!(path.is_file());
        let stored = InboxEntry::decode(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored, entry);

        let tmp_dir = tmp.path().join("inbox").join("session-1").join(".tmp");
        assert_eq!(std::fs::read_dir(&tmp_dir).unwrap().count(), 0);
    }

    #[test]
    fn modes_are_0700_and_0600() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        write_entry(tmp.path(), &sample_entry("session-1", &id)).unwrap();

        let session_dir = tmp.path().join("inbox").join("session-1");
        let session_mode = std::fs::metadata(&session_dir)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(session_mode, 0o700);

        let entry_path = session_dir.join(format!("{id}.json"));
        let entry_mode = std::fs::metadata(&entry_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(entry_mode, 0o600);
    }

    #[test]
    fn a_second_write_of_the_same_id_is_already_relayed() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let entry = sample_entry("session-1", &id);

        assert_eq!(
            write_entry(tmp.path(), &entry).unwrap(),
            WriteOutcome::Written
        );
        assert_eq!(
            write_entry(tmp.path(), &entry).unwrap(),
            WriteOutcome::AlreadyRelayed
        );
    }

    #[test]
    fn a_ledger_recorded_id_is_already_relayed_without_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let entry = sample_entry("session-1", &id);
        super::super::ledger::append_ledger(
            tmp.path(),
            "session-1",
            &super::super::ledger::LedgerRecord {
                id: id.clone(),
                kind: RequestKind::Memory,
                state: None,
                outcome: Some(super::super::ledger::LedgerOutcome::Applied),
                reason: None,
                at: Utc::now(),
            },
        )
        .unwrap();

        assert_eq!(
            write_entry(tmp.path(), &entry).unwrap(),
            WriteOutcome::AlreadyRelayed
        );
        let path = tmp
            .path()
            .join("inbox")
            .join("session-1")
            .join(format!("{id}.json"));
        assert!(!path.exists());
    }

    #[test]
    fn capacity_is_refused_at_the_pending_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("inbox").join("session-1");
        std::fs::create_dir_all(&session_dir).unwrap();
        for i in 0..MAX_PENDING_ENTRIES {
            std::fs::write(session_dir.join(format!("filler-{i}.json")), b"{}").unwrap();
        }

        let id = new_request_id();
        assert_eq!(
            write_entry(tmp.path(), &sample_entry("session-1", &id)).unwrap(),
            WriteOutcome::Capacity
        );
    }

    #[test]
    fn an_invalid_session_id_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let id = new_request_id();
        let entry = sample_entry("../escape", &id);
        assert!(write_entry(tmp.path(), &entry).is_err());
    }
}
