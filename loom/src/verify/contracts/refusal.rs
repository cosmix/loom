//! A contract writer's refused freeze, and the operator wait it ends in.
//!
//! `loom stage contracts freeze` runs inside the writer's sandbox, which
//! cannot write the state directory. When the freeze fails there, the CLI
//! leaves what it listed in the session's relay scratch directory, the one
//! place the sandbox writes and the host reads ([`record`]); an attempt that
//! gets through to the daemon removes it again ([`clear`]), so the file always
//! stands for the latest attempt. When the writer stops with it in place,
//! `loom-hooks/commit-guard.sh` runs `loom stage waiting`, which parks the
//! stage in `WaitingForInput` with the problems as its `review_reason`, the
//! reason `loom status` prints ([`standing`], [`park`]). A relayed freeze the
//! daemon refuses parks the same way from the inbox drain
//! ([`park_refused_relay`]): the relay told the writer to end its turn.
//!
//! Typing into the session continues the contract phase: the monitor moves a
//! waiting stage back to `Executing` once its session runs a tool, and so do
//! an accepted freeze and `loom stage resume`. Each clears the reason
//! ([`end_wait`]).
//!
//! The file is the agent's to write, so it is read as untrusted text: never
//! through a symlink, bounded, and flattened by the status collector before
//! it is shown. Forging one parks only the forger's own stage, which
//! `AskUserQuestion` already lets it do.

use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::path::Path;

use super::store::load_freeze;
use crate::fs::safe_read::open_regular_no_follow;
use crate::fs::session_files::load_session_exact;
use crate::models::session::SessionType;
use crate::models::stage::{Stage, StageStatus};
use crate::relay::{scratch_root_from_env, session_dir, validate_session_dir};
use crate::verify::transitions::update_stage;

/// The refusal's file name in the writer's scratch directory. Kept in step
/// with `loom-hooks/commit-guard.sh`, which looks for it when the writer stops.
pub const REFUSAL_FILE: &str = "contract-freeze-refused.txt";

/// Longest refusal read back; the rest of an oversized file is dropped.
const MAX_REFUSAL_BYTES: u64 = 16 * 1024;

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

/// The problems on record in `scratch_dir`, if its writer's latest freeze
/// attempt was refused.
fn load(scratch_dir: &Path) -> Result<Option<String>> {
    let Some(file) = open_regular_no_follow(scratch_dir, REFUSAL_FILE, libc::O_RDONLY)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.take(MAX_REFUSAL_BYTES)
        .read_to_end(&mut bytes)
        .context("failed to read the freeze refusal")?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Where a stage's contract writer stands when something asks the stage to
/// wait for the operator.
#[derive(Debug, PartialEq, Eq)]
pub enum Standing {
    /// Not a contract writer, or one with no refused freeze on record.
    Other,
    /// The writer's latest freeze was refused, for these problems.
    Refused(String),
    /// The contracts are frozen and the writer is being handed over; a wait
    /// would hold the handover up, which needs the stage `Executing`.
    Frozen,
}

/// `stage`'s standing, from its current session, its freeze record and the
/// writer's scratch directory. Evidence that cannot be read counts as none,
/// which leaves the plain wait every other session gets.
pub fn standing(work_dir: &Path, stage: &Stage) -> Standing {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let scratch_root = scratch_root_from_env();
    standing_in(work_dir, stage, scratch_root.as_deref().ok(), uid)
}

fn standing_in(work_dir: &Path, stage: &Stage, scratch_root: Option<&Path>, uid: u32) -> Standing {
    read_standing(work_dir, stage, scratch_root, uid).unwrap_or_else(|error| {
        tracing::warn!(
            stage_id = %stage.id,
            error = %format!("{error:#}"),
            "Cannot read the contract writer's freeze refusal; waiting without its reason"
        );
        Standing::Other
    })
}

fn read_standing(
    work_dir: &Path,
    stage: &Stage,
    scratch_root: Option<&Path>,
    uid: u32,
) -> Result<Standing> {
    let Some(session_id) = stage.session.as_deref() else {
        return Ok(Standing::Other);
    };
    let writer = load_session_exact(work_dir, session_id)?.filter(|session| {
        session.session_type == SessionType::Contract
            && session.stage_id.as_deref() == Some(stage.id.as_str())
    });
    if writer.is_none() {
        return Ok(Standing::Other);
    }
    if load_freeze(work_dir, &stage.id)?.is_some() {
        return Ok(Standing::Frozen);
    }
    let root = scratch_root.context("the relay scratch root cannot be determined")?;
    let scratch_dir = session_dir(root, session_id)?;
    if std::fs::symlink_metadata(&scratch_dir).is_err() {
        // A writer started without a relay scratch directory freezes over
        // the socket and reads the daemon's refusal itself.
        return Ok(Standing::Other);
    }
    validate_session_dir(&scratch_dir, session_id, uid)?;
    Ok(load(&scratch_dir)?.map_or(Standing::Other, Standing::Refused))
}

/// Park an `Executing` stage whose writer stopped on a refused freeze:
/// `WaitingForInput`, with `problems` in the reason `loom status` shows.
pub fn park(stage: &mut Stage, problems: &str) -> Result<()> {
    stage.try_mark_waiting_for_input()?;
    stage.review_reason = Some(reason(problems));
    Ok(())
}

/// Take a stage out of `WaitingForInput`; the reason it waited for no longer
/// holds.
pub fn end_wait(stage: &mut Stage) -> Result<()> {
    stage.try_mark_executing()?;
    stage.review_reason = None;
    Ok(())
}

/// Park the stage of a relayed freeze the daemon refused with `message`, if it
/// is still `Executing` under `session_id` with nothing frozen. The relay told
/// the writer to end its turn, so no one else would act on the refusal.
/// Returns whether the stage was parked.
pub fn park_refused_relay(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    message: &str,
) -> Result<bool> {
    if load_freeze(work_dir, stage_id)?.is_some() {
        return Ok(false);
    }
    let mut parked = false;
    update_stage(stage_id, work_dir, |stage| {
        if stage.status != StageStatus::Executing || stage.session.as_deref() != Some(session_id) {
            return Ok(());
        }
        parked = true;
        park(stage, message)
    })?;
    Ok(parked)
}

/// The operator-facing reason: every non-empty problem line, `;`-joined.
fn reason(problems: &str) -> String {
    let listed: Vec<&str> = problems
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    format!(
        "contract freeze refused; fix and freeze again: {}",
        listed.join("; ")
    )
}

#[cfg(test)]
#[path = "refusal_tests.rs"]
mod tests;
