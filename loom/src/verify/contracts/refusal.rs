//! A contract writer's refused freeze, kept until loom files it as a
//! `freeze-refusal` dispute.
//!
//! `loom stage contracts freeze` runs inside the writer's sandbox, which
//! cannot write the state directory. When the freeze fails there, the CLI
//! leaves what it listed in the session's relay scratch directory, the one
//! place the sandbox writes and the host reads ([`record`]); an attempt that
//! gets through to the daemon removes it again ([`clear`]), so the file always
//! stands for the latest attempt ([`local_refusal`]).
//!
//! A freeze the daemon refuses, at the relayed request or when it applies the
//! contract run's host job, is kept in `.loom/work/contracts/<stage>/refused.json`
//! instead ([`record_daemon_refusal`]), a file only the daemon writes. The
//! daemon's `dispute_refused_freezes` tick step files either one as a
//! `freeze-refusal` dispute for the stage's current contract writer: the daemon
//! record at once, the writer's file once the writer has been quiet for five
//! minutes. Nothing parks the stage: an adjudicator decides the refusal and a
//! fresh writer starts with the verdict.
//!
//! The scratch file is the agent's to write, so it is read as untrusted text:
//! never through a symlink, bounded, and labelled as the writer's claim when it
//! is filed.
//!
//! A contract writer may still wait for the operator through `AskUserQuestion`
//! like any session; typing into the session, an accepted freeze and
//! `loom stage resume` end that wait ([`end_wait`]).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::store::{canonical_work_dir, load_freeze};
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::safe_read::{is_not_found, open_regular_no_follow, read_bounded};
use crate::fs::session_files::load_session_exact;
use crate::models::session::SessionType;
use crate::models::stage::Stage;
use crate::relay::{session_dir, validate_session_dir};

/// The refusal's file name in the writer's scratch directory. Kept in step
/// with `loom-hooks/commit-guard.sh`, which looks for it when the writer stops.
pub const REFUSAL_FILE: &str = "contract-freeze-refused.txt";

/// Longest refusal read back; the rest of an oversized file is dropped.
const MAX_REFUSAL_BYTES: u64 = 16 * 1024;

/// Starts every problem of a filed refusal the writer's CLI reported, the
/// only mark of its source once the record is gone. The adjudication prompt
/// labels such problems as the writer's claim.
pub const WRITER_PREFIX: &str = "writer-reported: ";

/// Starts the first problem of a refusal no one's change caused: the host
/// run was interrupted or stopped, or its reports or binding could not be
/// read. The adjudication prompt tells the judge to have the freeze rerun.
pub const INFRASTRUCTURE_PREFIX: &str = "infrastructure: ";

/// `.loom/work/contracts/<stage>/refused.json`. The directory is the one
/// `store` keeps the freeze record in.
const CONTRACTS_DIR: &str = "contracts";
const DAEMON_REFUSAL_FILE: &str = "refused.json";
const MAX_DAEMON_REFUSAL_BYTES: usize = 256 * 1024;

/// Record `problems` as the outcome of the writer's latest freeze attempt.
pub fn record(scratch_dir: &Path, problems: &[String]) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(scratch_dir)
        .context("failed to create the freeze refusal in the scratch directory")?;
    file.write_all(problems.join("\n").as_bytes())
        .context("failed to write the freeze refusal")?;
    file.persist(scratch_dir.join(REFUSAL_FILE))
        .context("failed to record the freeze refusal")?;
    Ok(())
}

/// The latest attempt reached the daemon, so no refusal stands for it.
pub fn clear(scratch_dir: &Path) -> Result<()> {
    match std::fs::remove_file(scratch_dir.join(REFUSAL_FILE)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(error).context("failed to clear the freeze refusal")
        }
        _ => Ok(()),
    }
}

/// The refusal on record in `scratch_dir` and the time it was written, if
/// its writer's latest freeze attempt was refused.
fn load(scratch_dir: &Path) -> Result<Option<(String, SystemTime)>> {
    let Some(file) = open_regular_no_follow(scratch_dir, REFUSAL_FILE, libc::O_RDONLY)? else {
        return Ok(None);
    };
    let modified = file
        .metadata()
        .and_then(|metadata| metadata.modified())
        .context("failed to read the freeze refusal's modification time")?;
    let mut bytes = Vec::new();
    file.take(MAX_REFUSAL_BYTES)
        .read_to_end(&mut bytes)
        .context("failed to read the freeze refusal")?;
    Ok(Some((
        String::from_utf8_lossy(&bytes).into_owned(),
        modified,
    )))
}

/// The refusal writer `session_id` left in its scratch directory under
/// `scratch_root`: its non-empty problem lines and the file's modification
/// time. `None` when there is no refusal or no scratch directory (a writer
/// started without one freezes over the socket and reads the daemon's
/// refusal itself).
pub fn local_refusal(
    scratch_root: &Path,
    session_id: &str,
) -> Result<Option<(Vec<String>, SystemTime)>> {
    let scratch_dir = session_dir(scratch_root, session_id)?;
    if std::fs::symlink_metadata(&scratch_dir).is_err() {
        return Ok(None);
    }
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    validate_session_dir(&scratch_dir, session_id, uid)?;
    Ok(load(&scratch_dir)?.map(|(text, modified)| (problem_lines(&text), modified)))
}

/// Every non-empty, trimmed line of `text`.
pub fn problem_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Where a stage's contract writer stands when something asks the stage to
/// wait for the operator.
#[derive(Debug, PartialEq, Eq)]
pub enum Standing {
    /// Not a contract writer, or one whose contracts are not frozen yet.
    Other,
    /// The contracts are frozen and the writer is being handed over; a wait
    /// would hold the handover up, which needs the stage `Executing`.
    Frozen,
}

/// `stage`'s standing, from its current session and its freeze record.
/// Evidence that cannot be read counts as none, which leaves the plain wait
/// every other session gets.
pub fn standing(work_dir: &Path, stage: &Stage) -> Standing {
    read_standing(work_dir, stage).unwrap_or_else(|error| {
        tracing::warn!(
            stage_id = %stage.id,
            error = %format!("{error:#}"),
            "Cannot read the contract writer's freeze record; waiting as any session"
        );
        Standing::Other
    })
}

fn read_standing(work_dir: &Path, stage: &Stage) -> Result<Standing> {
    let Some(session_id) = stage.session.as_deref() else {
        return Ok(Standing::Other);
    };
    let writer = load_session_exact(work_dir, session_id)?.filter(|session| {
        session.session_type == SessionType::Contract
            && session.stage_id.as_deref() == Some(stage.id.as_str())
    });
    if writer.is_some() && load_freeze(work_dir, &stage.id)?.is_some() {
        return Ok(Standing::Frozen);
    }
    Ok(Standing::Other)
}

/// Take a stage out of `WaitingForInput`; the reason it waited for no longer
/// holds.
pub fn end_wait(stage: &mut Stage) -> Result<()> {
    stage.try_mark_executing()?;
    stage.review_reason = None;
    Ok(())
}

/// Who found the problems of a refusal on record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefusalSource {
    /// The daemon refused the freeze itself.
    Daemon,
    /// The writer's CLI refused it; the problems are the writer's own text.
    Writer,
}

/// `refused.json`: a refused freeze of the session `session_id`, waiting for
/// the tick step to file it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonRefusal {
    pub session_id: String,
    pub problems: Vec<String>,
    pub at: DateTime<Utc>,
    pub source: RefusalSource,
    /// The admission's reason when loom declined to file it; a declined
    /// refusal is never filed again.
    pub declined: Option<String>,
}

/// Record a freeze refused for `session_id` of `stage_id`, replacing any
/// earlier record: the latest refusal is the one to file. Written atomically;
/// `Err` when it could not be written, so the caller retries.
pub fn record_daemon_refusal(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    problems: &[String],
    source: RefusalSource,
) -> Result<()> {
    let record = DaemonRefusal {
        session_id: session_id.to_string(),
        problems: problems.to_vec(),
        at: Utc::now(),
        source,
        declined: None,
    };
    let dir = refusal_dir(work_dir, stage_id)?;
    locked_dir_update(&dir, || write_refusal(&dir, &record))
        .with_context(|| format!("failed to record the refused freeze of stage '{stage_id}'"))
}

/// The refusal on record for `stage_id`, if any.
pub fn daemon_refusal(work_dir: &Path, stage_id: &str) -> Result<Option<DaemonRefusal>> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let relative = Path::new(CONTRACTS_DIR)
        .join(stage_id)
        .join(DAEMON_REFUSAL_FILE);
    let bytes = match read_bounded(&root, &relative, MAX_DAEMON_REFUSAL_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .with_context(|| format!("invalid refused-freeze record for stage '{stage_id}'"))
}

/// Mark the refusal on record declined for `reason`, so it is never filed
/// again. A record that is gone is left gone.
pub fn mark_declined(work_dir: &Path, stage_id: &str, reason: &str) -> Result<()> {
    let dir = refusal_dir(work_dir, stage_id)?;
    locked_dir_update(&dir, || {
        let Some(mut record) = daemon_refusal(work_dir, stage_id)? else {
            return Ok(());
        };
        record.declined = Some(reason.to_string());
        write_refusal(&dir, &record)
    })
}

/// Remove the refusal on record: it was filed, escalated or is stale. One
/// already gone is no error.
pub fn take_daemon_refusal(work_dir: &Path, stage_id: &str) -> Result<()> {
    let dir = refusal_dir(work_dir, stage_id)?;
    locked_dir_update(&dir, || {
        match std::fs::remove_file(dir.join(DAEMON_REFUSAL_FILE)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error)
                .with_context(|| format!("failed to remove the refused freeze of '{stage_id}'")),
            _ => Ok(()),
        }
    })
}

fn refusal_dir(work_dir: &Path, stage_id: &str) -> Result<PathBuf> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    Ok(root.join(CONTRACTS_DIR).join(stage_id))
}

/// Write `record` under `dir`; the caller holds the directory lock.
fn write_refusal(dir: &Path, record: &DaemonRefusal) -> Result<()> {
    let json = serde_json::to_string_pretty(record)?;
    atomic_write_locked(&dir.join(DAEMON_REFUSAL_FILE), &json)
}

#[cfg(test)]
#[path = "refusal_tests.rs"]
mod tests;
