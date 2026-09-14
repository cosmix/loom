use super::claude_files::validate_current_binding;
use super::model::{
    CodexEvidence, CodexEvidenceKind, CodexEvidenceOutcome, CodexExecution, LifecycleProducer,
    LifecycleRecord, LifecycleState, WorkerIdentity, WorkerOutcome,
};
use super::validation::{fold_states, validate_safe_id};
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const MAX_DETAIL_BYTES: usize = 4096;

pub(super) fn validate_codex_record(
    work_dir: &Path,
    record: &LifecycleRecord,
) -> Result<CodexEvidence> {
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
        bail!("Codex producer has non-Codex identity")
    };
    for id in [
        stage_id,
        loom_session_id,
        parent_session_id,
        forwarder_agent_id,
        unit_id,
        invocation_id,
    ] {
        validate_safe_id(id)?;
    }
    validate_current_binding(work_dir, stage_id, loom_session_id)
        .context("validating Codex stage-session binding")?;
    ensure!(
        workspace_root.is_absolute(),
        "Codex workspace is not absolute"
    );
    ensure!(
        fs::canonicalize(workspace_root)? == *workspace_root,
        "Codex workspace is not canonical"
    );
    let evidence: CodexEvidence = serde_json::from_value(record.evidence.clone())?;
    match (&record.producer, execution) {
        (LifecycleProducer::CodexCompanion, CodexExecution::Companion { .. })
        | (LifecycleProducer::CodexDirect, CodexExecution::Direct { .. }) => {}
        _ => bail!("Codex producer and execution mode disagree"),
    }
    validate_codex_fields(record, execution, invocation_id, &evidence)?;
    ensure!(
        codex_event_id(record, &evidence)? == record.event_id,
        "Codex event id mismatch"
    );
    Ok(evidence)
}

fn validate_codex_fields(
    record: &LifecycleRecord,
    execution: &CodexExecution,
    invocation_id: &str,
    evidence: &CodexEvidence,
) -> Result<()> {
    ensure!(
        !evidence.requested_model.is_empty() && evidence.requested_model.len() <= 128,
        "invalid requested model"
    );
    ensure!(
        !evidence.requested_effort.is_empty() && evidence.requested_effort.len() <= 128,
        "invalid requested effort"
    );
    ensure!(
        !evidence.requested_model.as_bytes().contains(&0)
            && !evidence.requested_effort.as_bytes().contains(&0),
        "NUL in requested model or effort"
    );
    ensure!(
        evidence.invocation_id == invocation_id,
        "Codex invocation mismatch"
    );
    ensure!(
        evidence
            .detail
            .as_ref()
            .is_none_or(|value| value.len() <= MAX_DETAIL_BYTES),
        "Codex outcome detail exceeds cap"
    );
    match execution {
        CodexExecution::Companion { job_id } => validate_companion(job_id, evidence)?,
        CodexExecution::Direct {
            thread_id,
            tool_use_id,
        } => validate_direct(thread_id, tool_use_id, evidence)?,
    }
    validate_codex_state(record, evidence)
}

fn validate_companion(job_id: &str, evidence: &CodexEvidence) -> Result<()> {
    validate_safe_id(job_id)?;
    ensure!(
        evidence.job_id.as_deref() == Some(job_id),
        "Codex job id mismatch"
    );
    ensure!(
        evidence.tool_use_id.is_none(),
        "companion evidence has direct tool id"
    );
    Ok(())
}

fn validate_direct(thread_id: &str, tool_use_id: &str, evidence: &CodexEvidence) -> Result<()> {
    validate_safe_id(thread_id)?;
    validate_safe_id(tool_use_id)?;
    ensure!(
        evidence.thread_id.as_deref() == Some(thread_id),
        "Codex thread id mismatch"
    );
    ensure!(
        evidence.tool_use_id.as_deref() == Some(tool_use_id),
        "Codex tool-use id mismatch"
    );
    ensure!(
        evidence.job_id.is_none(),
        "direct evidence has companion job id"
    );
    Ok(())
}

fn validate_codex_state(record: &LifecycleRecord, evidence: &CodexEvidence) -> Result<()> {
    let expected = match record.state {
        LifecycleState::Running => CodexEvidenceOutcome::Running,
        LifecycleState::Completed => CodexEvidenceOutcome::Succeeded,
        LifecycleState::Failed => CodexEvidenceOutcome::Failed,
        LifecycleState::Cancelled => CodexEvidenceOutcome::Cancelled,
        LifecycleState::Unknown => CodexEvidenceOutcome::Unknown,
        LifecycleState::TurnFinished | LifecycleState::Idle => {
            bail!("invalid Codex lifecycle state")
        }
    };
    ensure!(
        evidence.outcome == expected,
        "Codex state and outcome disagree"
    );
    if evidence.evidence_kind == CodexEvidenceKind::Authorization {
        ensure!(
            record.state == LifecycleState::Running,
            "Codex authorization is terminal"
        );
    }
    if record.state == LifecycleState::Running {
        ensure!(
            evidence.terminal_at.is_none(),
            "running Codex evidence is terminal"
        );
    } else {
        validate_terminal_fields(record, evidence)?;
    }
    Ok(())
}

fn validate_terminal_fields(record: &LifecycleRecord, evidence: &CodexEvidence) -> Result<()> {
    ensure!(
        evidence.terminal_at.is_some(),
        "terminal Codex evidence lacks timestamp"
    );
    ensure!(
        evidence
            .turn_id
            .as_deref()
            .is_some_and(|value| validate_safe_id(value).is_ok()),
        "terminal Codex evidence lacks exact turn id"
    );
    ensure!(
        evidence
            .terminal_at
            .as_ref()
            .is_some_and(|timestamp| timestamp <= &record.observed_at),
        "Codex terminal timestamp follows observation"
    );
    if record.producer == LifecycleProducer::CodexCompanion {
        ensure!(
            evidence
                .thread_id
                .as_deref()
                .is_some_and(|value| validate_safe_id(value).is_ok()),
            "terminal companion evidence lacks exact thread id"
        );
    }
    Ok(())
}

pub(crate) fn codex_event_id(record: &LifecycleRecord, evidence: &CodexEvidence) -> Result<String> {
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
        bail!("not a Codex identity")
    };
    let execution_id = match execution {
        CodexExecution::Companion { job_id } => format!("companion:{job_id}"),
        CodexExecution::Direct {
            thread_id,
            tool_use_id,
        } => format!("direct:{thread_id}:{tool_use_id}"),
    };
    let workspace = workspace_root
        .to_str()
        .context("Codex workspace is not UTF-8")?;
    Ok(hash_codex_fields(
        record,
        evidence,
        workspace,
        &execution_id,
        [
            stage_id,
            loom_session_id,
            parent_session_id,
            forwarder_agent_id,
            unit_id,
            invocation_id,
        ],
    ))
}

fn hash_codex_fields(
    record: &LifecycleRecord,
    evidence: &CodexEvidence,
    workspace: &str,
    execution_id: &str,
    identity: [&str; 6],
) -> String {
    let terminal_at = evidence
        .terminal_at
        .as_ref()
        .map(chrono::DateTime::to_rfc3339)
        .unwrap_or_default();
    let mut canonical = b"loom.lifecycle.codex.v1".to_vec();
    let fields = [producer_name(record.producer)]
        .into_iter()
        .chain(identity)
        .chain([
            workspace,
            execution_id,
            state_name(record.state),
            &evidence.requested_model,
            &evidence.requested_effort,
            evidence_kind_name(evidence.evidence_kind),
            evidence.turn_id.as_deref().unwrap_or(""),
            &terminal_at,
            outcome_name(evidence.outcome),
        ]);
    for field in fields {
        canonical.push(0);
        canonical.extend_from_slice(field.as_bytes());
    }
    format!("sha256:{}", hex::encode(Sha256::digest(canonical)))
}

pub(super) fn codex_records_outcome(records: &[&LifecycleRecord]) -> WorkerOutcome {
    let authorizations: Vec<_> = records
        .iter()
        .filter(|record| evidence_kind(record) == Some(CodexEvidenceKind::Authorization))
        .collect();
    if authorizations.is_empty() {
        return WorkerOutcome::Unknown("missing Codex authorization".into());
    }
    let requested: HashSet<_> = authorizations
        .iter()
        .filter_map(|record| {
            serde_json::from_value::<CodexEvidence>(record.evidence.clone())
                .ok()
                .map(|value| (value.requested_model, value.requested_effort))
        })
        .collect();
    if requested.len() != 1 {
        return WorkerOutcome::Unknown("conflicting Codex authorization".into());
    }
    let requested = requested.into_iter().next();
    let all_agree = records.iter().all(|record| {
        serde_json::from_value::<CodexEvidence>(record.evidence.clone())
            .ok()
            .and_then(|value| {
                requested.as_ref().map(|known| {
                    known.0 == value.requested_model && known.1 == value.requested_effort
                })
            })
            == Some(true)
    });
    if !all_agree {
        return WorkerOutcome::Unknown("terminal evidence differs from authorization".into());
    }
    let observations: Vec<_> = records
        .iter()
        .filter(|record| evidence_kind(record) == Some(CodexEvidenceKind::Observation))
        .copied()
        .collect();
    if observations.is_empty() {
        return WorkerOutcome::Unknown("missing real Codex child evidence".into());
    }
    fold_states(&observations)
}

fn evidence_kind(record: &LifecycleRecord) -> Option<CodexEvidenceKind> {
    serde_json::from_value::<CodexEvidence>(record.evidence.clone())
        .ok()
        .map(|evidence| evidence.evidence_kind)
}

fn producer_name(producer: LifecycleProducer) -> &'static str {
    match producer {
        LifecycleProducer::ClaudeSubagentStop => "claude_subagent_stop",
        LifecycleProducer::ClaudeTeammateIdle => "claude_teammate_idle",
        LifecycleProducer::CodexCompanion => "codex_companion",
        LifecycleProducer::CodexDirect => "codex_direct",
    }
}

fn state_name(state: LifecycleState) -> &'static str {
    match state {
        LifecycleState::TurnFinished => "turn_finished",
        LifecycleState::Idle => "idle",
        LifecycleState::Running => "running",
        LifecycleState::Completed => "completed",
        LifecycleState::Failed => "failed",
        LifecycleState::Cancelled => "cancelled",
        LifecycleState::Unknown => "unknown",
    }
}

fn outcome_name(outcome: CodexEvidenceOutcome) -> &'static str {
    match outcome {
        CodexEvidenceOutcome::Running => "running",
        CodexEvidenceOutcome::Succeeded => "succeeded",
        CodexEvidenceOutcome::Failed => "failed",
        CodexEvidenceOutcome::Cancelled => "cancelled",
        CodexEvidenceOutcome::Unknown => "unknown",
    }
}

fn evidence_kind_name(kind: CodexEvidenceKind) -> &'static str {
    match kind {
        CodexEvidenceKind::Authorization => "authorization",
        CodexEvidenceKind::Observation => "observation",
    }
}
