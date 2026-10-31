//! Conservative resolver discovery across CLI and daemon launches.

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::session_files::load_session_exact;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};

use super::merge::read_merge_signal;

/// Find a live or uncertain resolver for `stage_id` under its admission lock.
/// The lock excludes the signal-to-record publication window. A `Spawning`
/// record remains a reservation even when it has no PID; a missing record is
/// stale because no backend launch can precede the record write.
pub fn find_live_merge_session_for_stage(
    stage_id: &str,
    work_dir: &Path,
) -> Result<Option<String>> {
    super::merge_launch::with_merge_resolver_lock(work_dir, stage_id, || {
        find_live_merge_session_for_stage_unlocked(stage_id, work_dir)
    })
}

/// The scan for a caller that already holds the admission lock.
pub(crate) fn find_live_merge_session_for_stage_unlocked(
    stage_id: &str,
    work_dir: &Path,
) -> Result<Option<String>> {
    let signal_ids = super::crud::list_signals(work_dir)?;
    for signal_id in &signal_ids {
        let (session_id, record) = match read_merge_signal(signal_id, work_dir) {
            Ok(Some(signal)) if signal.stage_id == stage_id => {
                let id = signal.session_id;
                let record = load_session_exact(work_dir, &id).with_context(|| {
                    format!("the session record of merge resolver {id} cannot be read")
                })?;
                (id, record)
            }
            Ok(_) => continue,
            Err(error) => {
                match attribute_unreadable_signal(signal_id, stage_id, work_dir, error)? {
                    Some(record) => (signal_id.clone(), Some(record)),
                    None => continue,
                }
            }
        };

        if record
            .as_ref()
            .is_some_and(|session| resolver_record_alive(session, work_dir))
        {
            return Ok(Some(session_id));
        }
        if let Err(error) = super::crud::remove_signal(signal_id, work_dir) {
            tracing::warn!(signal_id = %signal_id, %error, "Failed to remove stale merge signal");
        }
    }
    Ok(None)
}

/// A PID without saved identity is uncertain. A verified start-time mismatch
/// is dead even when another process has reused the recorded PID.
fn resolver_record_alive(session: &Session, work_dir: &Path) -> bool {
    match session.status {
        SessionStatus::Spawning => true,
        SessionStatus::Running | SessionStatus::Paused => !matches!(
            session_process_status(work_dir, session),
            SessionProcessStatus::Dead
        ),
        SessionStatus::Completed | SessionStatus::Crashed | SessionStatus::ContextExhausted => {
            matches!(
                session_process_status(work_dir, session),
                SessionProcessStatus::VerifiedAlive | SessionProcessStatus::Unverifiable
            )
        }
    }
}

/// Attribute a broken signal through its filename's exact session record.
fn attribute_unreadable_signal(
    signal_id: &str,
    stage_id: &str,
    work_dir: &Path,
    error: anyhow::Error,
) -> Result<Option<Session>> {
    let Some(record) = load_session_exact(work_dir, signal_id).ok().flatten() else {
        return Err(error.context(format!(
            "merge signal {signal_id} cannot be read, and no session record attributes it to a \
             stage"
        )));
    };
    let ours = record.session_type == SessionType::Merge
        && record
            .stage_id
            .as_deref()
            .is_none_or(|owner| owner == stage_id);
    Ok(ours.then_some(record))
}
