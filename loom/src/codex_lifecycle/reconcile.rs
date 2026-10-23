use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{anyhow, ensure, Result};
use chrono::{DateTime, Utc};

use crate::models::forward_receipt::is_safe_id;
use crate::models::forward_receipt::job_record::CompanionJob;
use crate::models::session::Session;
use crate::subagent_lifecycle::store::{append_locked, codex_event_id};
use crate::subagent_lifecycle::{
    replay, AppendOutcome, CodexEvidence, CodexEvidenceKind, CodexExecution, LifecycleRecord,
    LifecycleState, WorkerIdentity, WorkerOutcome, LIFECYCLE_VERSION,
};

use super::authorization::CodexAuthorization;
use super::direct::{self, DirectOutcome, StageReceipts};
use super::jobs::{classify_job, locate_job, outcome, JobObservation};
use super::ledger::{read_authorization_rows, read_lifecycle_records, LedgerRow};
use super::{codex_worker_identity, ReconcileEntry, ReconcileReport};

const MAX_ACTIVE_STAGES: usize = 64;
const MAX_ACTIVE_SESSIONS: usize = 256;

pub(super) fn reconcile_with_state_root(
    work_dir: &Path,
    active_sessions: &[Session],
    state_root: &Path,
) -> Result<ReconcileReport> {
    reconcile_at(work_dir, active_sessions, state_root, Utc::now())
}

/// `now` decides only whether an authorization with neither a forward receipt
/// nor a companion job is still inside [`direct::RECEIPT_WINDOW_SECS`].
pub(crate) fn reconcile_at(
    work_dir: &Path,
    active_sessions: &[Session],
    state_root: &Path,
    now: DateTime<Utc>,
) -> Result<ReconcileReport> {
    let active = active_stage_sessions(active_sessions);
    let mut report = ReconcileReport::default();
    for (stage_id, session_ids) in active.into_iter().take(MAX_ACTIVE_STAGES) {
        let path = work_dir
            .join("subagents")
            .join(&stage_id)
            .join("codex.jsonl");
        let rows = match read_authorization_rows(&path) {
            Ok(rows) => rows,
            Err(error) => {
                report
                    .entries
                    .push(ReconcileEntry::unknown(&stage_id, None, error.to_string()));
                continue;
            }
        };
        let receipts = StageReceipts::load(work_dir, &stage_id, now);
        reconcile_stage_rows(
            work_dir,
            state_root,
            &stage_id,
            &session_ids,
            rows,
            &receipts,
            &mut report,
        );
    }
    Ok(report)
}

pub(super) fn companion_outcome_with_state_root(
    work_dir: &Path,
    identity: &CodexAuthorization,
    state_root: &Path,
) -> WorkerOutcome {
    let result = validate_authorization_row(work_dir, identity)
        .and_then(|()| locate_job(identity, state_root));
    match result {
        Ok(job) => outcome(&job),
        Err(error) => WorkerOutcome::Unknown(error.to_string()),
    }
}

pub(super) fn validate_authorization_row(
    work_dir: &Path,
    expected: &CodexAuthorization,
) -> Result<()> {
    let path = work_dir
        .join("subagents")
        .join(&expected.stage_id)
        .join("codex.jsonl");
    let matches = read_authorization_rows(&path)?
        .into_iter()
        .filter(|row| matches!(row, LedgerRow::Authorization(value) if value.as_ref() == expected))
        .count();
    ensure!(
        matches == 1,
        "expected exactly one authorization row, found {matches}"
    );
    Ok(())
}

pub(super) fn has_correlated_lifecycle(
    work_dir: &Path,
    authorization: &CodexAuthorization,
) -> bool {
    let path = work_dir
        .join("subagents")
        .join(&authorization.stage_id)
        .join("lifecycle.jsonl");
    let Ok(records) = read_lifecycle_records(&path) else {
        return false;
    };
    let identities: HashSet<_> = records
        .iter()
        .filter_map(|record| correlated_identity(record, authorization))
        .collect();
    if identities.len() != 1 {
        return false;
    }
    let Some(identity) = identities.into_iter().next() else {
        return false;
    };
    replay(work_dir)
        .ok()
        .is_some_and(|index| !matches!(index.outcome(&identity), WorkerOutcome::Unknown(_)))
}

fn active_stage_sessions(sessions: &[Session]) -> BTreeMap<String, HashSet<String>> {
    let mut active: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    for session in sessions
        .iter()
        .take(MAX_ACTIVE_SESSIONS)
        .filter(|session| !session.status.is_terminal())
    {
        let Some(stage_id) = session.stage_id.as_deref() else {
            continue;
        };
        if is_safe_id(stage_id) && is_safe_id(&session.id) {
            active
                .entry(stage_id.into())
                .or_default()
                .insert(session.id.clone());
        }
    }
    active
}

fn reconcile_stage_rows(
    work_dir: &Path,
    state_root: &Path,
    stage_id: &str,
    session_ids: &HashSet<String>,
    rows: Vec<LedgerRow>,
    receipts: &StageReceipts,
    report: &mut ReconcileReport,
) {
    for row in rows {
        match row {
            LedgerRow::Authorization(authorization)
                if authorization.stage_id == stage_id
                    && session_ids.contains(&authorization.loom_session_id) =>
            {
                reconcile_one(work_dir, state_root, *authorization, receipts, report);
            }
            LedgerRow::Authorization(authorization) if authorization.stage_id != stage_id => {
                report.entries.push(ReconcileEntry::for_authorization(
                    &authorization,
                    WorkerOutcome::Unknown("authorization stage differs from ledger path".into()),
                    "authorization stage differs from ledger path".into(),
                ));
            }
            LedgerRow::Authorization(_) | LedgerRow::Legacy => {}
            LedgerRow::Invalid(detail) => {
                report
                    .entries
                    .push(ReconcileEntry::unknown(stage_id, None, detail));
            }
        }
    }
}

fn reconcile_one(
    work_dir: &Path,
    state_root: &Path,
    authorization: CodexAuthorization,
    receipts: &StageReceipts,
    report: &mut ReconcileReport,
) {
    let result = observe(state_root, &authorization, receipts).and_then(|observed| {
        observed
            .map(|(execution, observation)| {
                append_records(work_dir, &authorization, execution, &observation)
                    .map(|outcomes| (observation, outcomes))
            })
            .transpose()
    });
    match result {
        Ok(Some((observation, outcomes))) => {
            record_appends(report, &authorization, &observation, outcomes);
        }
        Ok(None) => report.entries.push(ReconcileEntry::for_authorization(
            &authorization,
            WorkerOutcome::Active,
            direct::AWAITING_RECEIPT.into(),
        )),
        Err(error) => report.entries.push(ReconcileEntry::for_authorization(
            &authorization,
            WorkerOutcome::Unknown(error.to_string()),
            error.to_string(),
        )),
    }
}

/// The execution an authorization's evidence names and what it observed, or
/// `None` while a row with no receipt and no companion job is in the window.
fn observe(
    state_root: &Path,
    authorization: &CodexAuthorization,
    receipts: &StageReceipts,
) -> Result<Option<(CodexExecution, JobObservation)>> {
    let routed = match receipts.receipts() {
        Ok(load) => direct::receipt_outcome(authorization, load),
        Err(detail) => DirectOutcome::Unknown(detail.to_owned()),
    };
    match routed {
        DirectOutcome::Direct(execution, observation) => Ok(Some((execution, observation))),
        DirectOutcome::Unknown(detail) => Err(anyhow!(detail)),
        DirectOutcome::Companion => companion_observation(locate_job(authorization, state_root)?),
        DirectOutcome::NoReceipt => match locate_job(authorization, state_root) {
            Ok(job) => companion_observation(job),
            Err(error) => {
                direct::receipt_window(authorization, &error, receipts.now).map(|()| None)
            }
        },
    }
}

fn companion_observation(job: CompanionJob) -> Result<Option<(CodexExecution, JobObservation)>> {
    let observation = classify_job(&job)?;
    Ok(Some((
        CodexExecution::Companion { job_id: job.id },
        observation,
    )))
}

fn record_appends(
    report: &mut ReconcileReport,
    authorization: &CodexAuthorization,
    observation: &JobObservation,
    outcomes: [AppendOutcome; 2],
) {
    let conflict = outcomes.contains(&AppendOutcome::Conflict);
    report.appended += outcomes
        .iter()
        .filter(|value| **value == AppendOutcome::Appended)
        .count();
    report.duplicates += outcomes
        .iter()
        .filter(|value| **value == AppendOutcome::Duplicate)
        .count();
    let external = if conflict {
        WorkerOutcome::Unknown("conflicting lifecycle event id".into())
    } else {
        observation_outcome(observation)
    };
    report.entries.push(ReconcileEntry::for_authorization(
        authorization,
        external,
        format!(
            "authorization={:?}, observation={:?}",
            outcomes[0], outcomes[1]
        ),
    ));
}

fn append_records(
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
    let authorization_outcome = append_locked(work_dir, &authorized)?;
    let observation_outcome = append_locked(work_dir, &observed)?;
    Ok([authorization_outcome, observation_outcome])
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

fn observation_outcome(observation: &JobObservation) -> WorkerOutcome {
    match observation.outcome {
        crate::subagent_lifecycle::CodexEvidenceOutcome::Running => WorkerOutcome::Active,
        crate::subagent_lifecycle::CodexEvidenceOutcome::Succeeded => WorkerOutcome::Succeeded,
        crate::subagent_lifecycle::CodexEvidenceOutcome::Failed => WorkerOutcome::Failed(
            observation
                .detail
                .clone()
                .unwrap_or_else(|| "Codex companion failed".into()),
        ),
        crate::subagent_lifecycle::CodexEvidenceOutcome::Cancelled => WorkerOutcome::Cancelled(
            observation
                .detail
                .clone()
                .unwrap_or_else(|| "Codex companion cancelled".into()),
        ),
        crate::subagent_lifecycle::CodexEvidenceOutcome::Unknown => {
            WorkerOutcome::Unknown("unknown companion outcome".into())
        }
    }
}

fn correlated_identity(
    record: &LifecycleRecord,
    authorization: &CodexAuthorization,
) -> Option<WorkerIdentity> {
    let WorkerIdentity::Codex {
        stage_id,
        loom_session_id,
        parent_session_id,
        forwarder_agent_id,
        unit_id,
        invocation_id,
        workspace_root,
        execution,
    } = &record.identity
    else {
        return None;
    };
    let evidence: CodexEvidence = serde_json::from_value(record.evidence.clone()).ok()?;
    let other_tool_use = matches!(execution, CodexExecution::Direct { tool_use_id, .. }
        if tool_use_id != &authorization.tool_use_id);
    (record.producer == direct::producer_of(&record.identity)
        && !other_tool_use
        && record.state == LifecycleState::Running
        && record.observed_at == authorization.authorized_at
        && evidence.evidence_kind == CodexEvidenceKind::Authorization
        && evidence.requested_model == authorization.model
        && evidence.requested_effort == authorization.effort
        && stage_id == &authorization.stage_id
        && loom_session_id == &authorization.loom_session_id
        && parent_session_id == &authorization.parent_session_id
        && forwarder_agent_id == &authorization.forwarder_agent_id
        && unit_id == &authorization.unit_id
        && invocation_id == &authorization.invocation_id
        && workspace_root == &authorization.workspace_root)
        .then(|| record.identity.clone())
}
