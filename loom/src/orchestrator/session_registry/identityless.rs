//! Judging a `Spawning` or `Running` record that has no PID identity.
//!
//! The spawn paths save a record before anything runs, and the wrapper script
//! a backend launches writes the PID file before it `exec`s the agent. A record
//! with no PID file either has no process or has one still to write it, so it
//! stays uncertain while that can happen and ends once it cannot:
//!
//! - older than [`ORPHAN_PROBE_GRACE_SECS`], measured from the later of the
//!   record and its wrapper script, a probe that finds nothing proves it dead.
//!   The record comes well before the wrapper when overlay reconcile or signal
//!   generation is slow. Orphan recovery and merge-writer retirement use this
//!   rule;
//! - a caller holding the stage's admission lock knows no launch is in flight,
//!   so a `Spawning` record with no wrapper script never reached a backend and
//!   has no process loom started. Manual mode leaves that record by design, and
//!   a crash between write-ahead and launch leaves the same one.
//!
//! An agent an operator started by hand leaves no evidence loom can read, so
//! it is not "alive" to these checks: the operator stops it, as the manual
//! start told them. Revisit if manual records gain an identity of their own.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::{wrapper_script_path, NativeBackend};

use super::LivenessProbe;

/// A session younger than this cannot be judged dead by a failed liveness
/// probe: its pid identity file and terminal window are written
/// asynchronously after spawn, so a probe run inside this window observes
/// "not written yet", not "gone".
pub(crate) const ORPHAN_PROBE_GRACE_SECS: i64 = 30;

/// Whether `session` is too young for a failed liveness probe to be treated
/// as evidence that it is dead.
pub(crate) fn too_young_to_judge(session: &Session, now: DateTime<Utc>) -> bool {
    let age = now.signed_duration_since(session.created_at);
    age < chrono::Duration::seconds(ORPHAN_PROBE_GRACE_SECS)
}

/// [`too_young_to_judge`], also counting from the wrapper script a launch wrote
/// just before starting it, when there is one.
pub(crate) fn launch_too_young_to_judge(
    work_dir: &Path,
    session: &Session,
    now: DateTime<Utc>,
) -> bool {
    let wrapper_written = || -> Option<DateTime<Utc>> {
        let (_, pid_key) = NativeBackend::window_title_and_pid_key(session)?;
        let path = wrapper_script_path(work_dir, &pid_key);
        let modified = std::fs::metadata(path).and_then(|meta| meta.modified());
        Some(modified.ok()?.into())
    };
    too_young_to_judge(session, now)
        || wrapper_written().is_some_and(|written| {
            now.signed_duration_since(written) < chrono::Duration::seconds(ORPHAN_PROBE_GRACE_SECS)
        })
}

/// Whether the caller holds the stage's admission lock, which every launch
/// holds from its write-ahead record through its backend's answer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Admission {
    Held,
    Open,
}

/// Whether `session`, which has no PID identity and which the backend's probe
/// did not find, can have no process.
pub(crate) fn identityless_record_is_dead(
    work_dir: &Path,
    session: &Session,
    admission: Admission,
    now: DateTime<Utc>,
) -> bool {
    !launch_too_young_to_judge(work_dir, session, now)
        || (admission == Admission::Held && launch_never_began(work_dir, session))
}

/// `prepare_session_launch` writes the wrapper before a backend starts it.
fn launch_never_began(work_dir: &Path, session: &Session) -> bool {
    session.status == SessionStatus::Spawning
        && NativeBackend::window_title_and_pid_key(session)
            .is_some_and(|(_, pid_key)| !wrapper_script_path(work_dir, &pid_key).exists())
}

/// Whether a process may still run for `session`: a probe that found it, or
/// one that cannot rule it out. A caller without the admission lock.
pub(crate) fn record_may_be_running(work_dir: &Path, session: &Session) -> bool {
    !matches!(
        LivenessProbe::resolve(work_dir).is_alive(work_dir, session, Admission::Open),
        Ok(false)
    )
}

/// Check one assigned session through the validated inventory, without the
/// stage's admission lock. A missing record or confirmed dead process is false;
/// malformed inventory, a mismatched owner, or a failed probe leaves ownership
/// unknown.
#[cfg(test)]
pub(crate) fn assigned_session_is_live(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
) -> Result<bool> {
    assigned_session_judged(work_dir, stage_id, session_id, Admission::Open)
}

/// [`assigned_session_is_live`] for a caller holding the stage's admission lock.
pub(crate) fn assigned_session_is_live_admitted(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
) -> Result<bool> {
    assigned_session_judged(work_dir, stage_id, session_id, Admission::Held)
}

fn assigned_session_judged(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    admission: Admission,
) -> Result<bool> {
    let sessions = super::in_progress::session_inventory(work_dir)?.strict_records()?;
    let Some(session) = sessions
        .into_iter()
        .find(|session| session.id == session_id)
    else {
        return Ok(false);
    };
    anyhow::ensure!(
        session.stage_id.as_deref() == Some(stage_id),
        "assigned session '{}' belongs to {:?}, not '{}'",
        session_id,
        session.stage_id,
        stage_id
    );
    LivenessProbe::resolve(work_dir)
        .is_alive(work_dir, &session, admission)
        .with_context(|| format!("probing assigned session '{session_id}'"))
}

/// [`super::live_sessions_for_stage`] for a caller holding the stage's
/// admission lock.
pub(crate) fn live_sessions_for_stage_admitted(
    work_dir: &Path,
    stage_id: &str,
) -> Result<Vec<Session>> {
    let probe = LivenessProbe::resolve(work_dir);
    live_sessions_judged(&probe, work_dir, stage_id, Admission::Held)
}

/// The in-progress records of `stage_id` whose process `probe` finds.
pub(super) fn live_sessions_judged(
    probe: &LivenessProbe,
    work_dir: &Path,
    stage_id: &str,
    admission: Admission,
) -> Result<Vec<Session>> {
    let mut live = Vec::new();
    for session in super::in_progress_sessions_for_stage(work_dir, stage_id)? {
        if probe
            .is_alive(work_dir, &session, admission)
            .with_context(|| format!("probing session '{}'", session.id))?
        {
            live.push(session);
        }
    }
    Ok(live)
}

#[cfg(test)]
#[path = "identityless_tests.rs"]
mod tests;
