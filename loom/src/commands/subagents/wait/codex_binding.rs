use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};

use crate::codex_lifecycle::{
    read_authorization_rows, read_lifecycle_records, CodexAuthorization, LedgerRow,
};
use crate::models::forward_receipt::job_record::read_companion_job;
use crate::models::forward_receipt::{
    is_safe_id, load_receipts, receipts_path, ForwardBackend, ForwardReceipt, ForwardState,
};
use crate::subagent_lifecycle::{
    replay, CodexExecution, LifecycleRecord, WorkerIdentity, WorkerOutcome,
};

/// Load the valid v2 Codex authorizations recorded for one wait stage.
pub(crate) fn load_wait_authorizations(
    work_dir: &Path,
    stage_id: &str,
) -> Result<Vec<CodexAuthorization>> {
    ensure!(is_safe_id(stage_id), "unsafe Codex authorization stage id");
    let path = work_dir
        .join("subagents")
        .join(stage_id)
        .join("codex.jsonl");
    let mut authorizations = Vec::new();
    for row in read_authorization_rows(&path)? {
        match row {
            LedgerRow::Authorization(row) => {
                ensure!(
                    row.stage_id == stage_id,
                    "authorization stage differs from journal path"
                );
                authorizations.push(*row);
            }
            LedgerRow::Invalid(error) => bail!("invalid Codex authorization row: {error}"),
            LedgerRow::Legacy => {}
        }
    }
    Ok(authorizations)
}

/// An exact Codex execution and the evidence that established its identity.
pub(crate) struct WaitCodexExecution {
    /// Lifecycle identity used by the wait engine.
    pub identity: WorkerIdentity,
    /// Stable name of the evidence source.
    pub source: String,
    /// Journal or job record that supplied the binding.
    pub path: PathBuf,
    /// Exact job or thread identifier.
    pub label: String,
}

/// Resolve exactly one authoritative Codex execution for an authorization.
pub(crate) fn resolve_wait_codex_execution(
    work_dir: &Path,
    stage_id: &str,
    authorization: &CodexAuthorization,
) -> Result<WaitCodexExecution> {
    ensure!(
        authorization.stage_id == stage_id,
        "authorization stage mismatch"
    );
    let mut matches = companion_matches(authorization)?;
    lifecycle_matches(work_dir, stage_id, authorization, &mut matches)?;
    receipt_matches(work_dir, stage_id, authorization, &mut matches)?;
    ensure!(
        matches.len() == 1,
        "expected exactly one Codex execution, found {}",
        matches.len()
    );
    matches.pop().context("missing Codex execution")
}

fn companion_matches(authorization: &CodexAuthorization) -> Result<Vec<WaitCodexExecution>> {
    let roots = match fs::read_dir(&authorization.effective_state_root) {
        Ok(roots) => roots,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("reading {}", authorization.effective_state_root.display())
            })
        }
    };
    let roots: Vec<_> = roots.take(257).collect::<std::io::Result<_>>()?;
    ensure!(roots.len() <= 256, "companion state root exceeds scan cap");
    let mut matches = Vec::new();
    for root in roots {
        if root.file_type()?.is_dir() {
            scan_jobs(authorization, &root.path().join("jobs"), &mut matches)?;
        }
    }
    Ok(matches)
}

fn scan_jobs(
    authorization: &CodexAuthorization,
    jobs: &Path,
    matches: &mut Vec<WaitCodexExecution>,
) -> Result<()> {
    let entries = match fs::read_dir(jobs) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", jobs.display())),
    };
    let entries: Vec<_> = entries.take(257).collect::<std::io::Result<_>>()?;
    ensure!(
        entries.len() <= 256,
        "companion jobs directory exceeds scan cap"
    );
    for entry in entries {
        inspect_job(authorization, entry.path(), matches)?;
    }
    Ok(())
}

fn inspect_job(
    authorization: &CodexAuthorization,
    path: PathBuf,
    matches: &mut Vec<WaitCodexExecution>,
) -> Result<()> {
    let Some(id) = companion_job_id(&path)? else {
        return Ok(());
    };
    let Ok(job) = read_companion_job(&path, &id) else {
        return Ok(());
    };
    let expected_session = authorization.encoded_session_id();
    if job.session_id.as_deref() != Some(expected_session.as_str()) {
        return Ok(());
    }
    job.validate_v1_0_6()?;
    ensure!(
        job.workspace_root.as_deref() == Some(authorization.workspace_root.as_path()),
        "companion workspace mismatch"
    );
    ensure!(
        job.job_class.as_deref() == Some("task") && job.write == Some(true),
        "companion job binding mismatch"
    );
    let request = job
        .request
        .as_ref()
        .context("companion task request missing")?;
    ensure!(
        request.cwd == authorization.workspace_root,
        "companion request cwd mismatch"
    );
    ensure!(
        request.model == authorization.model && request.effort == authorization.effort,
        "companion request mismatch"
    );
    let execution = CodexExecution::Companion { job_id: id.clone() };
    push_match(
        matches,
        authorization,
        execution,
        "codex_companion_job",
        path,
        &id,
    );
    Ok(())
}

fn companion_job_id(path: &Path) -> Result<Option<String>> {
    if path.extension().and_then(|value| value.to_str()) != Some("json")
        || !fs::symlink_metadata(path)?.file_type().is_file()
    {
        return Ok(None);
    }
    let Some(id) = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    Ok(is_safe_id(&id).then_some(id))
}

fn lifecycle_matches(
    work_dir: &Path,
    stage_id: &str,
    authorization: &CodexAuthorization,
    matches: &mut Vec<WaitCodexExecution>,
) -> Result<()> {
    let path = work_dir
        .join("subagents")
        .join(stage_id)
        .join("lifecycle.jsonl");
    let records = lifecycle_records(&path)?;
    if records.is_empty() {
        return Ok(());
    }
    let index = replay(work_dir)?;
    for record in records {
        if !identity_matches(&record.identity, authorization) {
            continue;
        }
        ensure!(
            !matches!(index.outcome(&record.identity), WorkerOutcome::Unknown(_)),
            "invalid Codex lifecycle evidence"
        );
        let execution = codex_execution(&record.identity).context("missing Codex execution")?;
        let label = execution_label(&execution);
        let source = execution_source(&execution, "codex_companion_lifecycle");
        push_match(
            matches,
            authorization,
            execution,
            source,
            path.clone(),
            &label,
        );
    }
    Ok(())
}

fn lifecycle_records(path: &Path) -> Result<Vec<LifecycleRecord>> {
    match read_lifecycle_records(path) {
        Ok(records) => Ok(records),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

fn receipt_matches(
    work_dir: &Path,
    stage_id: &str,
    authorization: &CodexAuthorization,
    matches: &mut Vec<WaitCodexExecution>,
) -> Result<()> {
    let path = receipts_path(work_dir, stage_id)?;
    let loaded = load_receipts(&path)?;
    ensure!(
        !loaded.truncated && loaded.malformed == 0,
        "damaged forward receipt journal"
    );
    for receipt in loaded
        .receipts
        .iter()
        .filter(|receipt| receipt_matches_authorization(receipt, authorization))
    {
        ensure!(
            receipt.state != ForwardState::Unknown,
            "invalid exact forward receipt"
        );
        let execution = match receipt.backend {
            ForwardBackend::Companion => CodexExecution::Companion {
                job_id: receipt.backend_id.clone(),
            },
            ForwardBackend::Direct => CodexExecution::Direct {
                thread_id: receipt.backend_id.clone(),
                tool_use_id: authorization.tool_use_id.clone(),
            },
        };
        let label = execution_label(&execution);
        let source = execution_source(&execution, "codex_forward_receipt");
        push_match(
            matches,
            authorization,
            execution,
            source,
            path.clone(),
            &label,
        );
    }
    Ok(())
}

fn receipt_matches_authorization(
    receipt: &ForwardReceipt,
    authorization: &CodexAuthorization,
) -> bool {
    receipt.identity.stage_id == authorization.stage_id
        && receipt.identity.loom_session_id == authorization.loom_session_id
        && receipt.identity.parent_session_id == authorization.parent_session_id
        && receipt.identity.agent_id == authorization.forwarder_agent_id
        && receipt.identity.tool_use_id == authorization.tool_use_id
        && receipt
            .model
            .as_deref()
            .is_none_or(|value| value == authorization.model)
        && receipt
            .effort
            .as_deref()
            .is_none_or(|value| value == authorization.effort)
}

fn push_match(
    matches: &mut Vec<WaitCodexExecution>,
    authorization: &CodexAuthorization,
    execution: CodexExecution,
    source: &str,
    path: PathBuf,
    label: &str,
) {
    let identity = codex_identity(authorization, execution);
    if matches
        .iter()
        .any(|candidate| candidate.identity == identity)
    {
        return;
    }
    matches.push(WaitCodexExecution {
        identity,
        source: source.into(),
        path,
        label: label.into(),
    });
}

fn codex_identity(authorization: &CodexAuthorization, execution: CodexExecution) -> WorkerIdentity {
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

fn identity_matches(identity: &WorkerIdentity, authorization: &CodexAuthorization) -> bool {
    let WorkerIdentity::Codex {
        stage_id,
        loom_session_id,
        parent_session_id,
        forwarder_agent_id,
        unit_id,
        invocation_id,
        workspace_root,
        execution,
    } = identity
    else {
        return false;
    };
    stage_id == &authorization.stage_id
        && loom_session_id == &authorization.loom_session_id
        && parent_session_id == &authorization.parent_session_id
        && forwarder_agent_id == &authorization.forwarder_agent_id
        && unit_id == &authorization.unit_id
        && invocation_id == &authorization.invocation_id
        && workspace_root == &authorization.workspace_root
        && !matches!(execution, CodexExecution::Direct { tool_use_id, .. } if tool_use_id != &authorization.tool_use_id)
}

fn codex_execution(identity: &WorkerIdentity) -> Option<CodexExecution> {
    match identity {
        WorkerIdentity::Codex { execution, .. } => Some(execution.clone()),
        _ => None,
    }
}

fn execution_label(execution: &CodexExecution) -> String {
    match execution {
        CodexExecution::Companion { job_id } => job_id.clone(),
        CodexExecution::Direct { thread_id, .. } => thread_id.clone(),
    }
}

fn execution_source<'a>(execution: &CodexExecution, companion: &'a str) -> &'a str {
    match execution {
        CodexExecution::Companion { .. } => companion,
        CodexExecution::Direct { .. } => "codex_direct_thread",
    }
}
