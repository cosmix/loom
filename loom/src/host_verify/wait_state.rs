//! What a session knows about the request it relayed: the scratch state file
//! that names it, the daemon's ledger entry for it, and the job record it
//! became (`wait` drives them).

use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use super::store::{load_current, load_job_record};
use super::wait::WaitSite;
use super::{JobRecord, JobState};
use crate::fs::inbox::{read_ledger, LedgerOutcome};

/// The request this session last relayed for a kind of job.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct RequestState {
    pub(super) request_id: String,
    pub(super) head: String,
    pub(super) tree_digest: String,
    pub(super) no_cache: bool,
    pub(super) criteria: Option<Vec<usize>>,
    pub(super) submitted_at: DateTime<Utc>,
}

pub(super) fn state_path(site: &WaitSite<'_>) -> Result<PathBuf> {
    crate::validation::validate_id(site.stage_id)?;
    let name = format!("{}-{}.json", site.stage_id, site.kind.name());
    Ok(site.scratch_dir.join("host-verify").join(name))
}

pub(super) fn read_state(path: &Path) -> Option<RequestState> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Atomic and private: a temp file in the same directory, renamed over.
pub(super) fn write_state(path: &Path, state: &RequestState) -> Result<()> {
    let dir = path.parent().context("request state has no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let _lock = lock_state(path)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(&serde_json::to_vec(state)?)?;
    file.persist(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// Delete the state only while it still holds `request_id`: every call of one
/// session for a stage and kind shares the file, and a call whose tree,
/// no-cache flag or criteria differ (a `--criterion` check beside a full one,
/// a rerun after an edit) replaces the request, so one waiter's end must not
/// discard a newer request another wrote.
pub(super) fn remove_state_if_owned(path: &Path, request_id: &str) -> Result<()> {
    remove_state_if_owned_inner(path, request_id, || {})
}

fn remove_state_if_owned_inner(
    path: &Path,
    request_id: &str,
    after_match: impl FnOnce(),
) -> Result<()> {
    let _lock = lock_state(path)?;
    if read_state(path).is_some_and(|state| state.request_id == request_id) {
        after_match();
        std::fs::remove_file(path).with_context(|| format!("cannot remove {}", path.display()))?;
    }
    Ok(())
}

fn lock_state(path: &Path) -> Result<std::fs::File> {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    let lock_path = PathBuf::from(name);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&lock_path)
        .with_context(|| format!("cannot open request state lock {}", lock_path.display()))?;
    let metadata = file.metadata()?;
    // SAFETY: geteuid has no preconditions and only reads the process identity.
    let uid = unsafe { libc::geteuid() };
    anyhow::ensure!(
        metadata.is_file() && metadata.uid() == uid,
        "request state lock {} is not an owned regular file",
        lock_path.display()
    );
    file.lock_exclusive()
        .with_context(|| format!("cannot lock request state {}", lock_path.display()))?;
    Ok(file)
}

#[cfg(test)]
#[path = "wait_state_tests.rs"]
mod tests;

/// The session's ledger outcome for `request_id`, with its note or reason.
pub(super) fn ledger_outcome(
    site: &WaitSite<'_>,
    request_id: &str,
) -> Result<Option<(LedgerOutcome, String)>> {
    // In a stage worktree `.loom/work` is a symlink to the main repository's
    // state directory, and the inbox reads refuse to follow symlinks.
    let root = site
        .work_dir
        .canonicalize()
        .with_context(|| format!("cannot resolve the state root {}", site.work_dir.display()))?;
    let ledger = read_ledger(&root, site.session_id)?;
    let settled = ledger
        .into_iter()
        .rev()
        .find(|record| record.id == request_id && record.outcome.is_some());
    Ok(settled.and_then(|record| {
        let reason = record.reason.unwrap_or_default();
        record.outcome.map(|outcome| match outcome {
            LedgerOutcome::UnknownAfterRestart => (
                outcome,
                "the daemon restarted before the request was applied".to_string(),
            ),
            _ => (outcome, reason),
        })
    }))
}

/// `host job <id> ...` names the job a request became.
pub(super) fn job_id_of(note: &str) -> Option<String> {
    let rest = note.strip_prefix("host job ")?;
    rest.split_whitespace().next().map(str::to_string)
}

/// The job record: `current.json` when it is this request's job, else the
/// job's own copy; a job whose directory was pruned reads as cancelled.
pub(super) fn find_job(
    site: &WaitSite<'_>,
    state: &RequestState,
    applied: Option<&str>,
) -> Result<Option<JobRecord>> {
    let (work_dir, stage_id) = (site.work_dir, site.stage_id);
    if let Some(current) = load_current(work_dir, stage_id)? {
        let named =
            applied == Some(current.id.as_str()) || current.requests.contains(&state.request_id);
        if named {
            return Ok(Some(current));
        }
    }
    let Some(job_id) = applied else {
        return Ok(None);
    };
    Ok(Some(match load_job_record(work_dir, stage_id, job_id)? {
        Some(record) => record,
        None => pruned(site, state, job_id),
    }))
}

fn pruned(site: &WaitSite<'_>, state: &RequestState, job_id: &str) -> JobRecord {
    JobRecord {
        id: job_id.to_string(),
        submission_order: 0,
        stage_id: site.stage_id.to_string(),
        session_id: site.session_id.to_string(),
        kind: site.kind,
        head: state.head.clone(),
        tree_digest: state.tree_digest.clone(),
        no_cache: state.no_cache,
        criteria: state.criteria.clone(),
        state: JobState::Cancelled,
        requests: vec![state.request_id.clone()],
        queued_at: state.submitted_at,
        started_at: None,
        finished_at: None,
        pid: None,
        detail: Some("the job's record was pruned".to_string()),
    }
}
