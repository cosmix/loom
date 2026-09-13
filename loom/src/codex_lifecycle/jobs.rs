use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::models::forward_receipt::is_safe_id;
use crate::models::forward_receipt::job_record::{read_companion_job, CompanionJob};
use crate::subagent_lifecycle::{CodexEvidenceOutcome, LifecycleState, WorkerOutcome};

use super::CodexAuthorization;

const MAX_JOB_FILES: usize = 256;

pub(super) struct JobObservation {
    pub state: LifecycleState,
    pub outcome: CodexEvidenceOutcome,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub terminal_at: Option<DateTime<Utc>>,
    pub detail: Option<String>,
}

pub(super) fn locate_job(
    authorization: &CodexAuthorization,
    expected_state_root: &Path,
) -> Result<CompanionJob> {
    authorization.validate_state_root(expected_state_root)?;
    let jobs_dir =
        workspace_state_dir(expected_state_root, &authorization.workspace_root)?.join("jobs");
    ensure_plain_directory(&jobs_dir)?;
    let entries: Vec<_> = fs::read_dir(&jobs_dir)?
        .take(MAX_JOB_FILES + 1)
        .collect::<std::io::Result<_>>()?;
    ensure!(
        entries.len() <= MAX_JOB_FILES,
        "companion jobs directory exceeds scan cap"
    );

    let expected_session = authorization.encoded_session_id();
    let mut matches = Vec::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let Some(job_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !is_safe_id(job_id) {
            continue;
        }
        let Ok(job) = read_companion_job(&path, job_id) else {
            continue;
        };
        if job.session_id.as_deref() == Some(&expected_session) {
            matches.push(job);
        }
    }
    ensure!(
        matches.len() == 1,
        "expected exactly one companion job, found {}",
        matches.len()
    );
    let job = matches.remove(0);
    job.validate_v1_0_6()?;
    validate_job_binding(&job, authorization)?;
    Ok(job)
}

pub(super) fn classify_job(job: &CompanionJob) -> Result<JobObservation> {
    match job.status.as_deref() {
        Some("queued") if job.phase.as_deref() == Some("queued") => Ok(JobObservation {
            state: LifecycleState::Running,
            outcome: CodexEvidenceOutcome::Running,
            thread_id: None,
            turn_id: None,
            terminal_at: None,
            detail: None,
        }),
        Some("running") => Ok(JobObservation {
            state: LifecycleState::Running,
            outcome: CodexEvidenceOutcome::Running,
            thread_id: None,
            turn_id: None,
            terminal_at: None,
            detail: None,
        }),
        Some("completed") if job.phase.as_deref() == Some("done") => {
            ensure!(job.result.is_some(), "completed companion job lacks result");
            terminal_observation(
                job,
                LifecycleState::Completed,
                CodexEvidenceOutcome::Succeeded,
            )
        }
        Some("failed") if job.phase.as_deref() == Some("failed") => {
            ensure!(
                job.result.is_some() || job.error_message.is_some(),
                "failed companion job lacks result or errorMessage"
            );
            terminal_observation(job, LifecycleState::Failed, CodexEvidenceOutcome::Failed)
        }
        Some("cancelled") if job.phase.as_deref() == Some("cancelled") => {
            ensure!(
                job.error_message.is_some(),
                "cancelled companion job lacks errorMessage"
            );
            terminal_observation(
                job,
                LifecycleState::Cancelled,
                CodexEvidenceOutcome::Cancelled,
            )
        }
        Some(status) => bail!("unsupported companion status/phase: {status}"),
        None => bail!("companion job has no status"),
    }
}

pub(super) fn outcome(job: &CompanionJob) -> WorkerOutcome {
    match classify_job(job) {
        Ok(observation) => match observation.outcome {
            CodexEvidenceOutcome::Running => WorkerOutcome::Active,
            CodexEvidenceOutcome::Succeeded => WorkerOutcome::Succeeded,
            CodexEvidenceOutcome::Failed => WorkerOutcome::Failed(
                observation
                    .detail
                    .unwrap_or_else(|| "Codex companion failed".into()),
            ),
            CodexEvidenceOutcome::Cancelled => WorkerOutcome::Cancelled(
                observation
                    .detail
                    .unwrap_or_else(|| "Codex companion cancelled".into()),
            ),
            CodexEvidenceOutcome::Unknown => {
                WorkerOutcome::Unknown("unknown companion outcome".into())
            }
        },
        Err(error) => WorkerOutcome::Unknown(error.to_string()),
    }
}

pub(super) fn workspace_state_dir(state_root: &Path, workspace: &Path) -> Result<PathBuf> {
    // Companion v1.0.6 scripts/lib/state.mjs:29-42.
    let canonical = workspace
        .to_str()
        .context("companion workspace root is not UTF-8")?;
    let basename = workspace
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace");
    let slug = workspace_slug(basename);
    let hash = hex::encode(Sha256::digest(canonical.as_bytes()));
    Ok(state_root.join(format!("{slug}-{}", &hash[..16])))
}

fn validate_job_binding(job: &CompanionJob, authorization: &CodexAuthorization) -> Result<()> {
    ensure!(
        job.workspace_root.as_deref() == Some(authorization.workspace_root.as_path()),
        "companion workspaceRoot mismatch"
    );
    ensure!(
        job.job_class.as_deref() == Some("task"),
        "companion jobClass is not task"
    );
    ensure!(
        job.write == Some(true),
        "companion job is not write-enabled"
    );
    let request = job
        .request
        .as_ref()
        .context("companion task request missing")?;
    ensure!(
        request.model == authorization.model,
        "companion request model mismatch"
    );
    ensure!(
        request.effort == authorization.effort,
        "companion request effort mismatch"
    );
    ensure!(
        request.job_id == job.id,
        "companion request job id mismatch"
    );
    Ok(())
}

fn terminal_observation(
    job: &CompanionJob,
    state: LifecycleState,
    outcome: CodexEvidenceOutcome,
) -> Result<JobObservation> {
    let thread_id = job
        .thread_id
        .clone()
        .context("terminal companion job lacks threadId")?;
    let turn_id = job
        .turn_id
        .clone()
        .context("terminal companion job lacks turnId")?;
    let terminal_at = job
        .completed_at
        .context("terminal companion job lacks completedAt")?;
    Ok(JobObservation {
        state,
        outcome,
        thread_id: Some(thread_id),
        turn_id: Some(turn_id),
        terminal_at: Some(terminal_at),
        detail: job.error_message.clone(),
    })
}

fn ensure_plain_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reading companion directory {}", path.display()))?;
    ensure!(
        metadata.file_type().is_dir(),
        "companion directory is not plain"
    );
    Ok(())
}

fn workspace_slug(source: &str) -> String {
    let mut slug = String::new();
    for character in source.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "workspace".into()
    } else {
        trimmed.into()
    }
}
