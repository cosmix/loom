//! Conservative resolver discovery across CLI and daemon launches.

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::session_files::{load_session_exact, mark_session_terminal_reason};
use crate::models::session::{Session, SessionExitReason, SessionStatus, SessionType};
use crate::orchestrator::session_registry::record_may_be_running;
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};

use super::merge::read_merge_signal;

/// Find a live or uncertain resolver for `stage_id` under its admission lock.
/// The lock excludes the signal-to-record publication window. A `Spawning`
/// record with no PID is a reservation until the registry's grace period ends
/// it, which a launch that died never outlives; a missing record is stale
/// because no backend launch can precede the record write.
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
                if record
                    .as_ref()
                    .is_some_and(|record| !record_matches(record, stage_id))
                {
                    continue;
                }
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
        retire_dead_reservation(record.as_ref(), work_dir);
    }
    Ok(None)
}

/// A launch that ended with no process leaves nothing to observe: close its
/// record, silently, since no crash happened to anyone's stage.
fn retire_dead_reservation(record: Option<&Session>, work_dir: &Path) {
    let Some(session) = record.filter(|session| session.status == SessionStatus::Spawning) else {
        return;
    };
    if let Err(error) = mark_session_terminal_reason(
        work_dir,
        &session.id,
        SessionStatus::ContextExhausted,
        SessionExitReason::Crashed,
    ) {
        tracing::warn!(session_id = %session.id, %error, "Failed to retire a dead resolver launch");
    }
}

/// A PID without saved identity is uncertain. A verified start-time mismatch
/// is dead even when another process has reused the recorded PID. No identity
/// at all is the registry's to judge: uncertain until its grace period ends.
fn resolver_record_alive(session: &Session, work_dir: &Path) -> bool {
    match session.status {
        SessionStatus::Spawning | SessionStatus::Running | SessionStatus::Paused => {
            match session_process_status(work_dir, session) {
                SessionProcessStatus::Dead => false,
                SessionProcessStatus::Missing => record_may_be_running(work_dir, session),
                SessionProcessStatus::VerifiedAlive | SessionProcessStatus::Unverifiable => true,
            }
        }
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
    Ok(record_matches(&record, stage_id).then_some(record))
}

fn record_matches(record: &Session, stage_id: &str) -> bool {
    record.session_type == SessionType::Merge
        && record
            .stage_id
            .as_deref()
            .is_none_or(|owner| owner == stage_id)
}

#[cfg(test)]
#[path = "merge_discovery_tests.rs"]
mod tests;
