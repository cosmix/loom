//! Host-side reconciliation of authorized Codex forwards: companion jobs, and
//! direct-mode forwards whose only evidence is the forward receipt.

use std::path::Path;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::models::session::Session;
use crate::subagent_lifecycle::{CodexExecution, WorkerIdentity, WorkerOutcome};

mod authorization;
mod direct;
mod jobs;
mod ledger;
mod progress;
mod reconcile;
mod reconcile_records;
mod token_threads;

pub use authorization::CodexAuthorization;
pub(crate) use direct::receipt_matches_authorization;
pub(crate) use ledger::{read_authorization_rows, read_lifecycle_records, LedgerRow};
pub(crate) use token_threads::authorized_threads;

/// An authorized forward from this exact stage attempt. `Some(false)` means
/// the worker existed but has no fresh evidence to hold stall recovery.
pub(crate) fn foreground_forward_active(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    since: DateTime<Utc>,
    budget: Duration,
    now: DateTime<Utc>,
) -> Option<bool> {
    let state_root = authorization::canonical_daemon_state_root().ok()?;
    forward_active_at(
        work_dir,
        stage_id,
        session_id,
        since,
        budget,
        now,
        &state_root,
    )
}

/// Every invocation authorized for this stage session, whatever its state. An
/// unreadable ledger names none, which costs a forward's launch only its
/// exclusion from the bounded hold any foreground command earns.
pub(crate) fn session_invocation_ids(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
) -> Vec<String> {
    if !crate::models::forward_receipt::is_safe_id(stage_id) {
        return Vec::new();
    }
    let path = work_dir
        .join("subagents")
        .join(stage_id)
        .join("codex.jsonl");
    let rows = ledger::read_authorization_rows(&path).unwrap_or_default();
    rows.into_iter()
        .filter_map(|row| match row {
            LedgerRow::Authorization(auth)
                if auth.stage_id == stage_id && auth.loom_session_id == session_id =>
            {
                Some(auth.invocation_id)
            }
            _ => None,
        })
        .collect()
}

fn forward_active_at(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    since: DateTime<Utc>,
    budget: Duration,
    now: DateTime<Utc>,
    state_root: &Path,
) -> Option<bool> {
    if !crate::models::forward_receipt::is_safe_id(stage_id)
        || !crate::models::forward_receipt::is_safe_id(session_id)
    {
        return None;
    }
    let path = work_dir
        .join("subagents")
        .join(stage_id)
        .join("codex.jsonl");
    let rows = ledger::read_authorization_rows(&path).ok()?;
    let receipts = direct::StageReceipts::load(work_dir, stage_id, now);
    let mut found = false;
    for row in rows {
        let LedgerRow::Authorization(auth) = row else {
            continue;
        };
        if auth.stage_id != stage_id
            || auth.loom_session_id != session_id
            || auth.authorized_at > now
            || auth.validate_state_root(state_root).is_err()
        {
            continue;
        }
        // `since` is the session's latest progress, which a parallel subagent's
        // tool call can move past a forward still running. Only a forward with
        // no live evidence is history at that point: it says nothing either way.
        found |= auth.authorized_at >= since;
        if forward_is_fresh(work_dir, &auth, &receipts, state_root, budget, now) {
            return Some(true);
        }
    }
    found.then_some(false)
}

/// Whether host observation of the launch the guard authorized shows progress.
/// A job file, its pid and its log are writable from the stage, so none of them
/// holds recovery without that launch.
fn forward_is_fresh(
    work_dir: &Path,
    auth: &CodexAuthorization,
    receipts: &direct::StageReceipts,
    state_root: &Path,
    budget: Duration,
    now: DateTime<Utc>,
) -> bool {
    use direct::DirectOutcome;
    let elapsed = now.signed_duration_since(auth.authorized_at);
    if elapsed.num_seconds() < 0 || elapsed.num_seconds() > direct::RECEIPT_WINDOW_SECS {
        return false;
    }
    match receipts
        .receipts()
        .map(|load| direct::receipt_outcome(auth, load))
    {
        // A direct START receipt proves neither ongoing progress nor the
        // launch identity of its worker.
        Ok(DirectOutcome::Direct(_, observation)) => {
            observation.state == crate::subagent_lifecycle::LifecycleState::Running
                && direct_worker_holds(work_dir, auth, budget, now)
        }
        Ok(DirectOutcome::Companion) => {
            companion_job_holds(work_dir, auth, state_root, budget, now)
        }
        // A foreground forward writes its first receipt when it ends, so nothing
        // names its lane yet; a planted job must not hide a direct worker.
        Ok(DirectOutcome::NoReceipt) => {
            companion_job_holds(work_dir, auth, state_root, budget, now)
                || direct_worker_holds(work_dir, auth, budget, now)
        }
        _ => false,
    }
}

/// Whether the direct worker of the launch `auth` names still holds recovery.
fn direct_worker_holds(
    work_dir: &Path,
    auth: &CodexAuthorization,
    budget: Duration,
    now: DateTime<Utc>,
) -> bool {
    crate::orchestrator::monitor::direct_worker::direct_worker_holds(
        work_dir,
        &auth.loom_session_id,
        &auth.invocation_id,
        auth.authorized_at,
        budget,
        now,
    )
}

/// Whether a companion job for `auth` logs progress while its launch awaits it.
fn companion_job_holds(
    work_dir: &Path,
    auth: &CodexAuthorization,
    state_root: &Path,
    budget: Duration,
    now: DateTime<Utc>,
) -> bool {
    jobs::locate_job(auth, state_root).is_ok_and(|job| {
        progress::fresh_companion_log(&job, state_root, budget, now.into())
            && crate::orchestrator::monitor::direct_worker::launch_awaits_job(
                work_dir,
                &auth.loom_session_id,
                &auth.invocation_id,
                &job.id,
            )
    })
}

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

    fn for_authorization(
        authorization: &CodexAuthorization,
        outcome: WorkerOutcome,
        detail: String,
    ) -> Self {
        Self {
            stage_id: authorization.stage_id.clone(),
            loom_session_id: Some(authorization.loom_session_id.clone()),
            unit_id: Some(authorization.unit_id.clone()),
            invocation_id: Some(authorization.invocation_id.clone()),
            outcome,
            detail,
        }
    }

    fn unknown(stage_id: &str, session_id: Option<String>, detail: String) -> Self {
        Self {
            stage_id: stage_id.into(),
            loom_session_id: session_id,
            unit_id: None,
            invocation_id: None,
            outcome: WorkerOutcome::Unknown(detail.clone()),
            detail,
        }
    }
}

/// The lifecycle identity of one authorized forward run as `execution`.
pub(crate) fn codex_worker_identity(
    authorization: &CodexAuthorization,
    execution: CodexExecution,
) -> WorkerIdentity {
    WorkerIdentity::Codex {
        stage_id: authorization.stage_id.clone(),
        loom_session_id: authorization.loom_session_id.clone(),
        parent_session_id: authorization.parent_session_id.clone(),
        forwarder_agent_id: authorization.forwarder_agent_id.clone(),
        unit_id: authorization.unit_id.clone(),
        invocation_id: authorization.invocation_id.clone(),
        workspace_root: authorization.workspace_root.clone(),
        execution,
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
#[path = "codex_lifecycle/foreground_tests.rs"]
mod foreground_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
pub(crate) use reconcile::reconcile_at;
#[cfg(test)]
pub(crate) use token_threads::threads_with_state_root;
#[cfg(test)]
mod tests;
