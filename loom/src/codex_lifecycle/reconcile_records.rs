//! The lifecycle records reconcile appends for one authorization: its
//! `running` authorization row and the observation it paired with it.

use std::path::Path;

use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Utc};

use crate::subagent_lifecycle::store::{append_all_locked, codex_event_id};
use crate::subagent_lifecycle::{
    AppendOutcome, CodexEvidence, CodexEvidenceKind, CodexExecution, LifecycleRecord,
    LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};

use super::authorization::CodexAuthorization;
use super::codex_worker_identity;
use super::direct::{self, UnknownFold};
use super::jobs::JobObservation;

pub(super) fn append_records(
    work_dir: &Path,
    authorization: &CodexAuthorization,
    execution: CodexExecution,
    observation: &JobObservation,
) -> Result<[AppendOutcome; 2]> {
    let identity = codex_worker_identity(authorization, execution);
    let authorized = lifecycle_record(
        &identity,
        authorization.authorized_at,
        LifecycleState::Running,
        direct::base_evidence(authorization, &identity),
    )?;
    let observed = observation_record(authorization, &identity, observation)?;
    // The fold is classified against the journal under the same lock the
    // append holds, so a concurrent reconcile cannot record the same direct
    // `unknown` twice.
    let outcomes =
        append_all_locked(
            work_dir,
            &[&authorized, &observed],
            |journal| match direct::unknown_fold(journal, &identity, observation.state) {
                UnknownFold::Record => Ok(true),
                UnknownFold::Recorded => Ok(false),
                UnknownFold::Unopened => bail!(
                    "{}",
                    observation
                        .detail
                        .as_deref()
                        .unwrap_or("unknown direct forward")
                ),
            },
        )?;
    outcomes
        .try_into()
        .map_err(|outcomes: Vec<AppendOutcome>| anyhow!("{} append outcomes", outcomes.len()))
}

fn observation_record(
    authorization: &CodexAuthorization,
    identity: &WorkerIdentity,
    observation: &JobObservation,
) -> Result<LifecycleRecord> {
    let observed_at = observation
        .terminal_at
        .unwrap_or(authorization.authorized_at);
    let evidence = CodexEvidence {
        evidence_kind: CodexEvidenceKind::Observation,
        thread_id: observation.thread_id.clone(),
        turn_id: observation.turn_id.clone(),
        terminal_at: observation.terminal_at,
        outcome: observation.outcome,
        detail: observation.detail.clone(),
        ..direct::base_evidence(authorization, identity)
    };
    lifecycle_record(identity, observed_at, observation.state, evidence)
}

fn lifecycle_record(
    identity: &WorkerIdentity,
    observed_at: DateTime<Utc>,
    state: LifecycleState,
    evidence: CodexEvidence,
) -> Result<LifecycleRecord> {
    let mut record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: direct::producer_of(identity),
        identity: identity.clone(),
        observed_at,
        state,
        evidence: serde_json::to_value(&evidence)?,
    };
    record.event_id = codex_event_id(&record, &evidence)?;
    Ok(record)
}
