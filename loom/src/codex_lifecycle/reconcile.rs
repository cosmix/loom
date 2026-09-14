use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{ensure, Result};

use crate::models::forward_receipt::is_safe_id;
use crate::models::session::Session;
use crate::subagent_lifecycle::store::{append_locked, codex_event_id};
use crate::subagent_lifecycle::{
    replay, AppendOutcome, CodexEvidence, CodexEvidenceKind, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, WorkerOutcome, LIFECYCLE_VERSION,
};

use super::authorization::CodexAuthorization;
use super::jobs::{classify_job, locate_job, outcome, JobObservation};
use super::ledger::{read_authorization_rows, read_lifecycle_records, LedgerRow};
use super::{ReconcileEntry, ReconcileReport};

const MAX_ACTIVE_STAGES: usize = 64;
const MAX_ACTIVE_SESSIONS: usize = 256;

pub(super) fn reconcile_with_state_root(
    work_dir: &Path,
    active_sessions: &[Session],
    state_root: &Path,
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
                    .push(unknown_entry(&stage_id, None, error.to_string()));
                continue;
            }
        };
        reconcile_stage_rows(
            work_dir,
            state_root,
            &stage_id,
            &session_ids,
            rows,
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

fn validate_authorization_row(work_dir: &Path, expected: &CodexAuthorization) -> Result<()> {
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
    report: &mut ReconcileReport,
) {
    for row in rows {
        match row {
            LedgerRow::Authorization(authorization)
                if authorization.stage_id == stage_id
                    && session_ids.contains(&authorization.loom_session_id) =>
            {
                reconcile_one(work_dir, state_root, *authorization, report);
            }
            LedgerRow::Authorization(authorization) if authorization.stage_id != stage_id => {
                report.entries.push(entry_for(
                    &authorization,
                    WorkerOutcome::Unknown("authorization stage differs from ledger path".into()),
                    "authorization stage differs from ledger path".into(),
                ));
            }
            LedgerRow::Authorization(_) | LedgerRow::Legacy => {}
            LedgerRow::Invalid(detail) => {
                report.entries.push(unknown_entry(stage_id, None, detail));
            }
        }
    }
}

fn reconcile_one(
    work_dir: &Path,
    state_root: &Path,
    authorization: CodexAuthorization,
    report: &mut ReconcileReport,
) {
    let result = locate_job(&authorization, state_root).and_then(|job| {
        let observation = classify_job(&job)?;
        append_records(work_dir, &authorization, &job.id, &observation)
            .map(|outcomes| (observation, outcomes))
    });
    match result {
        Ok((observation, outcomes)) => {
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
                observation_outcome(&observation)
            };
            report.entries.push(entry_for(
                &authorization,
                external,
                format!(
                    "authorization={:?}, observation={:?}",
                    outcomes[0], outcomes[1]
                ),
            ));
        }
        Err(error) => report.entries.push(entry_for(
            &authorization,
            WorkerOutcome::Unknown(error.to_string()),
            error.to_string(),
        )),
    }
}

fn append_records(
    work_dir: &Path,
    authorization: &CodexAuthorization,
    job_id: &str,
    observation: &JobObservation,
) -> Result<[AppendOutcome; 2]> {
    let identity = worker_identity(authorization, job_id);
    let authorized = authorization_record(authorization, &identity, job_id)?;
    let observed = observation_record(authorization, &identity, job_id, observation)?;
    let authorization_outcome = append_locked(work_dir, &authorized)?;
    let observation_outcome = append_locked(work_dir, &observed)?;
    Ok([authorization_outcome, observation_outcome])
}

fn authorization_record(
    authorization: &CodexAuthorization,
    identity: &WorkerIdentity,
    job_id: &str,
) -> Result<LifecycleRecord> {
    let evidence = CodexEvidence {
        evidence_kind: CodexEvidenceKind::Authorization,
        requested_model: authorization.model.clone(),
        requested_effort: authorization.effort.clone(),
        invocation_id: authorization.invocation_id.clone(),
        job_id: Some(job_id.into()),
        thread_id: None,
        turn_id: None,
        tool_use_id: None,
        terminal_at: None,
        outcome: crate::subagent_lifecycle::CodexEvidenceOutcome::Running,
        detail: None,
    };
    lifecycle_record(
        identity,
        authorization.authorized_at,
        LifecycleState::Running,
        evidence,
    )
}

fn observation_record(
    authorization: &CodexAuthorization,
    identity: &WorkerIdentity,
    job_id: &str,
    observation: &JobObservation,
) -> Result<LifecycleRecord> {
    let observed_at = observation
        .terminal_at
        .unwrap_or(authorization.authorized_at);
    let evidence = CodexEvidence {
        evidence_kind: CodexEvidenceKind::Observation,
        requested_model: authorization.model.clone(),
        requested_effort: authorization.effort.clone(),
        invocation_id: authorization.invocation_id.clone(),
        job_id: Some(job_id.into()),
        thread_id: observation.thread_id.clone(),
        turn_id: observation.turn_id.clone(),
        tool_use_id: None,
        terminal_at: observation.terminal_at,
        outcome: observation.outcome,
        detail: observation.detail.clone(),
    };
    lifecycle_record(identity, observed_at, observation.state, evidence)
}

fn lifecycle_record(
    identity: &WorkerIdentity,
    observed_at: chrono::DateTime<chrono::Utc>,
    state: LifecycleState,
    evidence: CodexEvidence,
) -> Result<LifecycleRecord> {
    let mut record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: String::new(),
        producer: LifecycleProducer::CodexCompanion,
        identity: identity.clone(),
        observed_at,
        state,
        evidence: serde_json::to_value(&evidence)?,
    };
    record.event_id = codex_event_id(&record, &evidence)?;
    Ok(record)
}

fn worker_identity(authorization: &CodexAuthorization, job_id: &str) -> WorkerIdentity {
    WorkerIdentity::Codex {
        stage_id: authorization.stage_id.clone(),
        loom_session_id: authorization.loom_session_id.clone(),
        parent_session_id: authorization.parent_session_id.clone(),
        forwarder_agent_id: authorization.forwarder_agent_id.clone(),
        unit_id: authorization.unit_id.clone(),
        invocation_id: authorization.invocation_id.clone(),
        workspace_root: authorization.workspace_root.clone(),
        execution: CodexExecution::Companion {
            job_id: job_id.into(),
        },
    }
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

fn entry_for(
    authorization: &CodexAuthorization,
    outcome: WorkerOutcome,
    detail: String,
) -> ReconcileEntry {
    ReconcileEntry {
        stage_id: authorization.stage_id.clone(),
        loom_session_id: Some(authorization.loom_session_id.clone()),
        unit_id: Some(authorization.unit_id.clone()),
        invocation_id: Some(authorization.invocation_id.clone()),
        outcome,
        detail,
    }
}

fn unknown_entry(stage_id: &str, session_id: Option<String>, detail: String) -> ReconcileEntry {
    ReconcileEntry {
        stage_id: stage_id.into(),
        loom_session_id: session_id,
        unit_id: None,
        invocation_id: None,
        outcome: WorkerOutcome::Unknown(detail.clone()),
        detail,
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
        execution: CodexExecution::Companion { .. },
    } = &record.identity
    else {
        return None;
    };
    let evidence: CodexEvidence = serde_json::from_value(record.evidence.clone()).ok()?;
    (record.producer == LifecycleProducer::CodexCompanion
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
