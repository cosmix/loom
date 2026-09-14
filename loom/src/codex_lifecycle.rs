//! Host-side reconciliation of authorized Codex companion jobs.

use std::path::Path;
use std::time::{Duration, SystemTime};

use anyhow::Result;

use crate::models::session::Session;
use crate::subagent_lifecycle::WorkerOutcome;

mod authorization;
mod jobs;
mod ledger;
mod progress;
mod reconcile;

pub use authorization::CodexAuthorization;
pub(crate) use ledger::{read_authorization_rows, read_lifecycle_records, LedgerRow};

#[derive(Debug, Default)]
pub struct ReconcileReport {
    pub entries: Vec<ReconcileEntry>,
    pub appended: usize,
    pub duplicates: usize,
}

#[derive(Debug)]
pub struct ReconcileEntry {
    pub stage_id: String,
    pub loom_session_id: Option<String>,
    pub unit_id: Option<String>,
    pub invocation_id: Option<String>,
    pub outcome: WorkerOutcome,
    pub detail: String,
}

impl ReconcileEntry {
    pub fn is_unknown(&self) -> bool {
        matches!(&self.outcome, WorkerOutcome::Unknown(_))
    }
}

/// Reconcile authorized companion jobs for the supplied active sessions.
pub fn reconcile_codex_jobs(
    work_dir: &Path,
    active_sessions: &[Session],
) -> Result<ReconcileReport> {
    let state_root = authorization::canonical_daemon_state_root()?;
    reconcile::reconcile_with_state_root(work_dir, active_sessions, &state_root)
}

/// Read the exact v1.0.6 companion job bound to an authorization.
pub fn companion_outcome(work_dir: &Path, identity: &CodexAuthorization) -> WorkerOutcome {
    let state_root = match authorization::canonical_daemon_state_root() {
        Ok(root) => root,
        Err(error) => return WorkerOutcome::Unknown(error.to_string()),
    };
    reconcile::companion_outcome_with_state_root(work_dir, identity, &state_root)
}

/// Read the exact v1.0.6 companion job bound to an authorization once, and
/// classify it: the terminal record when the ledger already has one, else
/// direct-observation progress evidence for a job still running.
///
/// Locating the job is an up-to-256-file directory scan, so callers that need
/// both the terminal outcome and progress evidence (a bounded wait polling a
/// running job every cycle) should use this instead of pairing
/// [`companion_outcome`] with a second lookup.
pub fn companion_outcome_with_progress(
    work_dir: &Path,
    identity: &CodexAuthorization,
    budget: Duration,
    now: SystemTime,
) -> WorkerOutcome {
    let state_root = match authorization::canonical_daemon_state_root() {
        Ok(root) => root,
        Err(error) => return WorkerOutcome::Unknown(error.to_string()),
    };
    let job = match reconcile::validate_authorization_row(work_dir, identity)
        .and_then(|()| jobs::locate_job(identity, &state_root))
    {
        Ok(job) => job,
        Err(error) => return WorkerOutcome::Unknown(error.to_string()),
    };
    let terminal = jobs::outcome(&job);
    if !matches!(terminal, WorkerOutcome::Active) {
        return terminal;
    }
    let prefix = format!("codex worker {}: ", identity.unit_id);
    match progress::companion_progress(&job, budget, now) {
        progress::CompanionProgress::Running => WorkerOutcome::Active,
        // A gone process is definitive, not merely stalled: the job cannot
        // reach a terminal record without one.
        progress::CompanionProgress::Dead(detail) => {
            WorkerOutcome::Failed(format!("{prefix}{detail}"))
        }
        progress::CompanionProgress::Stalled(detail) => {
            WorkerOutcome::Stalled(format!("{prefix}{detail}"))
        }
    }
}

pub(crate) fn has_correlated_lifecycle(work_dir: &Path, identity: &CodexAuthorization) -> bool {
    reconcile::has_correlated_lifecycle(work_dir, identity)
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
