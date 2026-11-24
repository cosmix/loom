//! Stop-hook decision for an unattended stage session with no pending work.

use std::ffi::OsStr;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::target::non_empty_env;
use crate::fs::safe_fs::{flock_exclusive, safe_open_dirfd};
use crate::fs::safe_read::{is_not_found, read_bounded_at};
use crate::fs::safe_replace::publish_in_dir;
use crate::fs::session_files::load_session_exact;
use crate::fs::work_dir::WorkDir;
use crate::host_verify::store::load_current;
use crate::models::{session::SessionType, stage::StageStatus};
use crate::orchestrator::monitor::{heartbeat_path, read_heartbeat};
use crate::orchestrator::Orchestrator;
use crate::plan_session::spool::read_payload;
use crate::subagent_lifecycle::store::ChildDisposition;
use crate::validation::validate_id;

const MARKER_NAME: &str = ".idle-stop";
const MAX_MARKER_BYTES: usize = 256;

/// How many times one session is told to reach a recorded exit.
pub(crate) const MAX_IDLE_STOP_BLOCKS: u32 = 3;

/// What the hook knows when the main agent of a session stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StopFacts {
    /// The stage is `Executing` and names this session as its current one.
    pub owns_executing_stage: bool,
    /// The session runs the stage's work (a stage or knowledge session), and
    /// is no contract writer, adjudicator or merge resolver.
    pub stage_session: bool,
    /// `Some(true)`: no subagent or worker of this session is active.
    /// `Some(false)`: one is. `None`: the evidence could not be read.
    pub children_quiet: Option<bool>,
    /// A host job (check, completion, freeze) of the stage is not terminal.
    pub host_job_live: bool,
    /// This is the first block, or the session heartbeat timestamp changed.
    pub worked_since_last_block: bool,
    /// Blocks this session has already received from this check.
    pub blocks_so_far: u32,
}

/// The reason to block the stop with, or `None` to let the session stop.
pub(crate) fn decide(stage_id: &str, facts: &StopFacts) -> Option<String> {
    if !facts.owns_executing_stage
        || !facts.stage_session
        || facts.children_quiet != Some(true)
        || facts.host_job_live
        || !facts.worked_since_last_block
        || facts.blocks_so_far >= MAX_IDLE_STOP_BLOCKS
    {
        return None;
    }

    Some(format!(
        "Stage {stage_id} is still Executing and this session runs unattended: nobody reads a final message. \
         End in a recorded state. (1) Finish: commit, then `loom stage complete {stage_id}`. \
         (2) A wrong check, contract, finding or integrity event: file the dispute that names it; \
         a refused commit does not stop a filing. (3) Something only a person can give \
         (a credential, a host install, a decision, a denied permission): `loom stage block {stage_id} \
         \"<what is needed and why>\"`, or AskUserQuestion when one answer unblocks you. \
         If a background command or agent of yours is still running, wait for it; \
         this notice does not repeat until the session records a new heartbeat."
    ))
}

/// `loom hook idle-stop`: reads the Stop payload on stdin, prints one
/// `{"decision":"block","reason":...}` object or nothing. Always `Ok`.
pub fn emit_idle_stop() -> Result<()> {
    if let Some(line) = idle_stop_line() {
        let _ = writeln!(std::io::stdout().lock(), "{line}");
    }
    Ok(())
}

fn idle_stop_line() -> Option<String> {
    let stage_id = non_empty_env("LOOM_STAGE_ID")?;
    validate_id(&stage_id).ok()?;
    let payload = read_payload(std::io::stdin().lock())?;
    if !matches!(
        payload.as_object()?.get("stop_hook_active"),
        None | Some(Value::Bool(false))
    ) {
        return None;
    }

    let session_id = non_empty_env("LOOM_SESSION_ID")?;
    let scratch_dir = non_empty_env("LOOM_SCRATCH_DIR")?;
    let work_dir = WorkDir::new(non_empty_env("LOOM_WORK_DIR")?).ok()?;
    block_line(
        work_dir.root(),
        &stage_id,
        &session_id,
        Path::new(&scratch_dir),
    )
}

fn block_line(work_dir: &Path, stage_id: &str, session_id: &str, scratch: &Path) -> Option<String> {
    let stage = crate::verify::load_stage(stage_id, work_dir).ok()?;
    let owns_executing_stage = stage.id == stage_id
        && stage.status == StageStatus::Executing
        && stage.session.as_deref() == Some(session_id);
    let stage_session = load_session_exact(work_dir, session_id)
        .ok()
        .flatten()
        .is_some_and(|session| {
            matches!(
                session.session_type,
                SessionType::Stage | SessionType::Knowledge
            )
        });
    if !owns_executing_stage || !stage_session {
        return None;
    }
    let children_quiet = children_quiet(work_dir, stage_id, session_id);
    let host_job_live = match load_current(work_dir, stage_id) {
        Ok(job) => job.is_some_and(|job| !job.state.is_terminal()),
        Err(_) => true,
    };

    // Lock the directory so another Stop cannot count the same heartbeat while
    // the marker is being replaced.
    let scratch = safe_open_dirfd(scratch).ok()?;
    flock_exclusive(&scratch).ok()?;
    let heartbeat = read_heartbeat(&heartbeat_path(work_dir, stage_id)).ok()?;
    if heartbeat.session_id != session_id || heartbeat.stage_id != stage_id {
        return None;
    }
    let timestamp = heartbeat.timestamp;
    let (blocks_so_far, worked_since_last_block) = marker_facts(&scratch, timestamp);
    let facts = StopFacts {
        owns_executing_stage,
        stage_session,
        children_quiet,
        host_job_live,
        worked_since_last_block,
        blocks_so_far,
    };
    let reason = decide(stage_id, &facts)?;
    write_marker(&scratch, blocks_so_far + 1, timestamp).ok()?;
    Some(json!({"decision": "block", "reason": reason}).to_string())
}

fn children_quiet(work_dir: &Path, stage_id: &str, session_id: &str) -> Option<bool> {
    match Orchestrator::session_child_disposition_in(work_dir, stage_id, session_id) {
        ChildDisposition::NoChildren => Some(true),
        ChildDisposition::Active => Some(false),
        ChildDisposition::Unknown(_) => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Marker {
    count: u32,
    timestamp: DateTime<Utc>,
}

fn read_marker(scratch: &OwnedFd) -> Result<Option<Marker>> {
    let bytes = match read_bounded_at(scratch, Path::new(MARKER_NAME), MAX_MARKER_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut lines = std::str::from_utf8(&bytes)?.lines();
    let count = lines
        .next()
        .context("missing idle-stop block count")?
        .parse()?;
    let timestamp = lines
        .next()
        .context("missing idle-stop timestamp")?
        .parse()?;
    anyhow::ensure!(lines.next().is_none(), "extra lines in idle-stop marker");
    Ok(Some(Marker { count, timestamp }))
}

fn marker_facts(scratch: &OwnedFd, timestamp: DateTime<Utc>) -> (u32, bool) {
    match read_marker(scratch) {
        Ok(None) => (0, true),
        Ok(Some(marker)) => (marker.count, marker.timestamp != timestamp),
        Err(_) => (MAX_IDLE_STOP_BLOCKS, false),
    }
}

fn write_marker(scratch: &OwnedFd, count: u32, timestamp: DateTime<Utc>) -> Result<()> {
    let content = format!("{count}\n{}\n", timestamp.to_rfc3339());
    publish_in_dir(
        scratch,
        OsStr::new(MARKER_NAME),
        content.as_bytes(),
        0o600,
        false,
    )
}

#[cfg(test)]
#[path = "idle_stop_tests.rs"]
mod tests;
