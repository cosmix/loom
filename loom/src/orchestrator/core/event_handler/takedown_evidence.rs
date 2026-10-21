//! What a stage takedown accepts as evidence about its agents: a session that
//! started with no PID identity is never confirmed dead, and the stage's
//! assigned session is always among the agents it takes down.

use std::path::Path;

use anyhow::{bail, Result};

use crate::fs::session_files::load_session_exact;
use crate::models::session::Session;

/// Count `session` a survivor when it had no PID identity evidence before the
/// takedown; returns whether it did.
pub(super) fn retain_missing_identity(
    missing: bool,
    session: &Session,
    survivors: &mut Vec<String>,
) -> bool {
    if !missing {
        return false;
    }
    eprintln!(
        "Warning: Session '{}' had no PID identity evidence before takedown; refusing to treat \
         absence as confirmed death",
        session.id
    );
    survivors.push(session.id.clone());
    true
}

/// Make sure `agents` holds the stage's assigned session, loading its exact
/// record when no other source found it.
pub(super) fn include_assigned_session(
    agents: &mut Vec<Session>,
    stage_id: &str,
    assigned_id: &str,
    work_dir: &Path,
) -> Result<()> {
    if let Some(assigned) = agents.iter().find(|session| session.id == assigned_id) {
        anyhow::ensure!(
            assigned.stage_id.as_deref() == Some(stage_id),
            "assigned session '{}' belongs to stage {:?}, not '{}'",
            assigned.id,
            assigned.stage_id,
            stage_id
        );
        return Ok(());
    }

    let Some(assigned) = load_session_exact(work_dir, assigned_id)? else {
        bail!(
            "stage '{stage_id}' assigns session '{assigned_id}', but no exact session record \
             exists; refusing to re-queue without durable proof that its writer is gone"
        );
    };
    anyhow::ensure!(
        assigned.stage_id.as_deref() == Some(stage_id),
        "assigned session '{}' belongs to stage {:?}, not '{}'",
        assigned.id,
        assigned.stage_id,
        stage_id
    );
    // Terminal is workflow state, not process-death evidence. Completion is
    // persisted before the agent finishes its merge/teardown path, so a
    // restarted daemon must still probe and take down the exact record.
    agents.push(assigned);
    Ok(())
}
