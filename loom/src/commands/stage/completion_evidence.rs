use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde_json::Value;

use crate::daemon::{send_request, user_credential as completion_credential, Request, Response};
use crate::handoff::{
    check_definition_hash, record_attempt_handoff, CompletionAttemptEvidence, CompletionPhase,
    CriterionResult, EnvironmentFact, MergeOutcome, VerificationCheckpoint,
    COMPLETION_EVIDENCE_VERSION, MAX_EVIDENCE_BYTES, MAX_TEXT_LEN, STAGE_ENVIRONMENT_POLICY,
};
use crate::models::session::Session;
use crate::models::stage::Stage;

pub mod broker;

pub const EVIDENCE_RECORD_PREFIX: &str = "LOOM_CONTROL_EVIDENCE_V1 ";
pub const EVIDENCE_EOF_MARKER: &str = "LOOM_CONTROL_EVIDENCE_EOF";
pub const OUTCOME_PREFIX: &str = "LOOM_CONTROL_OUTCOME ";
pub const TOOL_STATUS_ENV: &str = "LOOM_CONTROL_TOOL_STATUS";
pub const MAX_BROKER_INPUT_BYTES: usize = 4 * 1024 * 1024;

pub fn fresh_nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub fn pinned_command(loom_bin: &Path, stage_id: &str) -> String {
    let absolute = if loom_bin.is_absolute() {
        loom_bin.to_path_buf()
    } else {
        std::env::current_dir()
            .expect("current directory must be available while building the pinned command")
            .join(loom_bin)
    };
    format!("{} stage complete {stage_id}", absolute.display())
}

pub fn verified_evidence(
    stage: &Stage,
    session_id: &str,
    commit: String,
    exact_command: String,
) -> CompletionAttemptEvidence {
    let mut criteria = stage
        .acceptance
        .iter()
        .enumerate()
        .map(|(index, _)| CriterionResult {
            id: format!("acceptance-{index}"),
            passed: true,
        })
        .collect::<Vec<_>>();
    if stage.has_any_goal_checks() {
        criteria.push(CriterionResult {
            id: "goal-backward".to_string(),
            passed: true,
        });
    }
    evidence(stage, session_id, commit, exact_command, criteria)
}

pub fn diagnostic_evidence(
    stage: &Stage,
    session_id: &str,
    commit: String,
    exact_command: String,
    phase: CompletionPhase,
    diagnostic: Option<&str>,
) -> CompletionAttemptEvidence {
    let mut evidence = evidence(stage, session_id, commit, exact_command, Vec::new());
    evidence.phase = phase;
    evidence.diagnostic_first_line = bounded_diagnostic(diagnostic);
    evidence
}

pub fn with_boundary_failure(
    evidence: &CompletionAttemptEvidence,
    phase: CompletionPhase,
    failure_code: &str,
    diagnostic: Option<&str>,
) -> CompletionAttemptEvidence {
    let mut failed = evidence.clone();
    failed.phase = phase;
    failed.external_failure_code = Some(failure_code.to_string());
    failed.diagnostic_first_line = bounded_diagnostic(diagnostic);
    failed.observed_at = observed_at();
    failed
}

pub fn format_evidence_record(evidence: &CompletionAttemptEvidence) -> Result<String> {
    evidence.validate().context("invalid completion evidence")?;
    let json = serde_json::to_string(evidence).context("serializing completion evidence")?;
    ensure!(
        json.len() <= MAX_EVIDENCE_BYTES,
        "evidence JSON exceeds maximum bytes"
    );
    Ok(format!(
        "{EVIDENCE_RECORD_PREFIX}{json}\n{EVIDENCE_EOF_MARKER}\n"
    ))
}

pub fn parse_evidence_record(output: &str) -> Result<CompletionAttemptEvidence> {
    ensure!(
        output.len() <= MAX_BROKER_INPUT_BYTES,
        "broker input exceeds maximum bytes"
    );
    let trimmed = output.trim_end();
    let lines = trimmed.lines().collect::<Vec<_>>();
    ensure!(
        lines.last().copied() == Some(EVIDENCE_EOF_MARKER),
        "EOF marker is missing or is not the last line"
    );
    ensure!(
        lines[..lines.len() - 1]
            .iter()
            .all(|line| *line != EVIDENCE_EOF_MARKER),
        "EOF marker appears before the last line"
    );
    let record_indexes = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.starts_with(EVIDENCE_RECORD_PREFIX).then_some(index))
        .collect::<Vec<_>>();
    ensure!(
        !record_indexes.is_empty(),
        "completion evidence record is missing"
    );
    ensure!(
        record_indexes.len() == 1,
        "multiple completion evidence records found"
    );
    let record_index = record_indexes[0];
    ensure!(
        record_index + 2 == lines.len(),
        "completion evidence record must immediately precede EOF marker"
    );
    let json = &lines[record_index][EVIDENCE_RECORD_PREFIX.len()..];
    ensure!(
        json.len() <= MAX_EVIDENCE_BYTES,
        "evidence JSON exceeds maximum bytes"
    );
    let value: Value = serde_json::from_str(json).context("invalid completion evidence JSON")?;
    reject_unknown_fields(&value)?;
    let evidence: CompletionAttemptEvidence =
        serde_json::from_value(value).context("invalid completion evidence schema")?;
    evidence.validate().context("invalid completion evidence")?;
    Ok(evidence)
}

pub fn validate_against(
    evidence: &CompletionAttemptEvidence,
    stage: &Stage,
    session_id: &str,
    commit: &str,
    exact_command: &str,
) -> Result<()> {
    evidence.validate().context("invalid completion evidence")?;
    ensure!(
        evidence.stage_id == stage.id,
        "completion evidence stage mismatch"
    );
    ensure!(
        evidence.session_id == session_id,
        "completion evidence session mismatch"
    );
    ensure!(
        evidence.commit == commit,
        "completion evidence commit mismatch"
    );
    ensure!(
        evidence.check_definition_hash == check_definition_hash(stage),
        "completion evidence check definition hash mismatch"
    );
    ensure!(
        evidence.exact_command == exact_command,
        "completion evidence command mismatch"
    );
    ensure!(
        evidence.phase == CompletionPhase::VerifiedPendingAck,
        "completion evidence phase is not verified_pending_ack"
    );
    ensure!(
        evidence.external_failure_code.is_none(),
        "verified completion evidence has an external failure code"
    );
    ensure!(
        evidence.verification.all_passed(),
        "completion evidence verification did not pass"
    );
    Ok(())
}

pub fn send_to_daemon(evidence: &CompletionAttemptEvidence, work_dir: &Path) -> Result<Response> {
    let request = Request::RecordCompletionEvidence {
        auth_token: completion_credential(work_dir),
        stage_id: evidence.stage_id.clone(),
        session_id: evidence.session_id.clone(),
        evidence: Box::new(evidence.clone()),
    };
    send_request(work_dir, &request)
}

pub fn record_host_fallback(
    session: &Session,
    stage: &Stage,
    evidence: CompletionAttemptEvidence,
    work_dir: &Path,
) -> Result<(PathBuf, MergeOutcome)> {
    record_attempt_handoff(session, stage, &evidence, work_dir)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordRoute {
    Daemon,
    HostFallback,
}

pub fn record_evidence(
    session: &Session,
    stage: &Stage,
    evidence: &CompletionAttemptEvidence,
    work_dir: &Path,
) -> Result<RecordRoute> {
    match send_to_daemon(evidence, work_dir) {
        Ok(Response::Ok) => Ok(RecordRoute::Daemon),
        Err(_) => {
            record_host_fallback(session, stage, evidence.clone(), work_dir)?;
            Ok(RecordRoute::HostFallback)
        }
        Ok(Response::Error { message }) => bail!("daemon rejected completion evidence: {message}"),
        Ok(Response::AuthenticationFailed) => {
            bail!("daemon rejected completion evidence credential")
        }
        Ok(other) => bail!("unexpected completion evidence response: {other:?}"),
    }
}

fn evidence(
    stage: &Stage,
    session_id: &str,
    commit: String,
    exact_command: String,
    criteria: Vec<CriterionResult>,
) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session_id.to_string(),
        commit,
        check_definition_hash: check_definition_hash(stage),
        exact_command,
        evidence_nonce: fresh_nonce(),
        verification: VerificationCheckpoint {
            criteria,
            environment_policy: STAGE_ENVIRONMENT_POLICY.to_string(),
            environment: vec![EnvironmentFact {
                name: "TARGET_OS".to_string(),
                value: std::env::consts::OS.to_string(),
            }],
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: None,
        diagnostic_first_line: None,
        observed_at: observed_at(),
        attestation: None,
    }
}

fn bounded_diagnostic(diagnostic: Option<&str>) -> Option<String> {
    diagnostic.map(|text| {
        text.lines()
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control())
            .take(MAX_TEXT_LEN)
            .collect()
    })
}

fn observed_at() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn reject_unknown_fields(value: &Value) -> Result<()> {
    const EVIDENCE: &[&str] = &[
        "version",
        "stage_id",
        "session_id",
        "commit",
        "check_definition_hash",
        "exact_command",
        "evidence_nonce",
        "verification",
        "phase",
        "external_failure_code",
        "diagnostic_first_line",
        "observed_at",
    ];
    let object = checked_object(value, "completion evidence")?;
    ensure_known(object, EVIDENCE, "completion evidence")?;
    let verification = object
        .get("verification")
        .ok_or_else(|| anyhow::anyhow!("completion evidence verification is missing"))?;
    reject_unknown_verification_fields(verification)
}

fn reject_unknown_verification_fields(value: &Value) -> Result<()> {
    const VERIFICATION: &[&str] = &["criteria", "environment_policy", "environment"];
    const CRITERION: &[&str] = &["id", "passed"];
    const ENVIRONMENT: &[&str] = &["name", "value"];
    let object = checked_object(value, "verification")?;
    ensure_known(object, VERIFICATION, "verification")?;
    for criterion in object
        .get("criteria")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        ensure_known(
            checked_object(criterion, "criterion")?,
            CRITERION,
            "criterion",
        )?;
    }
    for fact in object
        .get("environment")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        ensure_known(
            checked_object(fact, "environment fact")?,
            ENVIRONMENT,
            "environment fact",
        )?;
    }
    Ok(())
}

fn checked_object<'a>(value: &'a Value, name: &str) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("{name} must be a JSON object"))
}

fn ensure_known(
    object: &serde_json::Map<String, Value>,
    expected: &[&str],
    name: &str,
) -> Result<()> {
    if let Some(field) = object.keys().find(|key| !expected.contains(&key.as_str())) {
        bail!("unknown {name} field `{field}`");
    }
    Ok(())
}

#[cfg(test)]
#[path = "completion_evidence/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "completion_evidence/broker_tests.rs"]
mod broker_tests;
