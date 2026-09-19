use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use uuid::Uuid;

use crate::codex_lifecycle::CodexAuthorization;
use crate::models::forward_receipt::is_safe_id;
use crate::subagent_lifecycle::WorkerIdentity;

use super::super::ledger::StartedAgentTypeIndex;
use super::super::resolve::project_slug;
use super::model::{
    resolve_claude_transcript, BoundWorker, CodexAuthority, EvidenceReference, WaitIdentity,
    WorkerKind, WorkerSpec,
};

const MAX_PARENT_DIRS: usize = 512;

pub struct IdentityScope {
    pub work_dir: PathBuf,
    pub canonical_repo: PathBuf,
    pub canonical_worktree: PathBuf,
    pub projects_root: PathBuf,
    pub stage_id: String,
    pub loom_session_id: String,
}

/// Bind the requested workers and return the canonical work directory
/// validated as part of that same environment snapshot.
pub fn resolve_from_environment_with_work_dir(
    workers: &[String],
    parent: Option<&str>,
) -> Result<(WaitIdentity, PathBuf)> {
    let specs = parse_worker_set(workers)?;
    let scope = scope_from_environment()?;
    let identity = resolve(&scope, specs, parent)?;
    Ok((identity, scope.work_dir))
}

pub fn resolve(
    scope: &IdentityScope,
    specs: Vec<WorkerSpec>,
    requested_parent: Option<&str>,
) -> Result<WaitIdentity> {
    validate_scope(scope)?;
    let starts = StartedAgentTypeIndex::load(Some(&scope.work_dir));
    let authorizations =
        super::codex_binding::load_wait_authorizations(&scope.work_dir, &scope.stage_id)?;
    let parent = common_parent(scope, &specs, requested_parent, &starts, &authorizations)?;
    let mut bound = Vec::with_capacity(specs.len());
    for spec in specs {
        bound.push(bind_worker(scope, spec, &parent, &starts, &authorizations)?);
    }
    Ok(WaitIdentity {
        canonical_repo: scope.canonical_repo.clone(),
        canonical_worktree: scope.canonical_worktree.clone(),
        stage_id: scope.stage_id.clone(),
        loom_session_id: scope.loom_session_id.clone(),
        parent_session_id: parent,
        workers: bound,
    })
}

pub fn parse_worker_set(workers: &[String]) -> Result<Vec<WorkerSpec>> {
    ensure!(
        !workers.is_empty(),
        "watch now requires --worker claude:<agent-id> or --worker codex:<unit-id>"
    );
    let mut specs = BTreeSet::new();
    let mut ids = HashSet::new();
    for value in workers {
        let spec = value.parse::<WorkerSpec>()?;
        ensure!(specs.insert(spec.clone()), "duplicate worker selector");
        ensure!(ids.insert(spec.id), "conflicting worker id aliases");
    }
    Ok(specs.into_iter().collect())
}

fn scope_from_environment() -> Result<IdentityScope> {
    let stage_id = required_safe_env("LOOM_STAGE_ID")?;
    let loom_session_id = required_safe_env("LOOM_SESSION_ID")?;
    let work_dir = canonical_env_dir("LOOM_WORK_DIR")?;
    let session = crate::fs::session_files::load_session_exact(&work_dir, &loom_session_id)?
        .context("Loom session record is missing")?;
    ensure!(
        session.stage_id.as_deref() == Some(&stage_id),
        "Loom session belongs to another stage"
    );
    let worktree = session
        .worktree_path
        .context("Loom session has no worktree identity")?;
    let canonical_worktree =
        fs::canonicalize(worktree).context("canonicalizing session worktree")?;
    validate_current_worktree(&canonical_worktree)?;
    let stage = crate::verify::load_stage(&stage_id, &work_dir)?;
    ensure!(
        stage.session.as_deref() == Some(&loom_session_id),
        "stage is not owned by this Loom session"
    );
    let canonical_repo = crate::git::worktree::find_repo_root_from_cwd(&canonical_worktree)
        .context("resolving canonical repository")?;
    let home = dirs::home_dir().context("home directory is unavailable")?;
    Ok(IdentityScope {
        work_dir,
        canonical_repo: fs::canonicalize(canonical_repo)?,
        canonical_worktree,
        projects_root: home.join(".claude/projects"),
        stage_id,
        loom_session_id,
    })
}

fn validate_scope(scope: &IdentityScope) -> Result<()> {
    ensure!(is_safe_id(&scope.stage_id), "unsafe stage id");
    ensure!(is_safe_id(&scope.loom_session_id), "unsafe Loom session id");
    ensure!(
        scope.work_dir.is_absolute(),
        "work directory is not absolute"
    );
    ensure!(
        scope.canonical_repo.is_absolute(),
        "repository is not absolute"
    );
    ensure!(
        scope.canonical_worktree.is_absolute(),
        "worktree is not absolute"
    );
    Ok(())
}

fn required_safe_env(name: &str) -> Result<String> {
    let value = std::env::var(name).with_context(|| format!("{name} is required"))?;
    ensure!(is_safe_id(&value), "{name} is empty or unsafe");
    Ok(value)
}

fn canonical_env_dir(name: &str) -> Result<PathBuf> {
    let raw = std::env::var_os(name).with_context(|| format!("{name} is required"))?;
    let path = fs::canonicalize(raw).with_context(|| format!("canonicalizing {name}"))?;
    ensure!(path.is_dir(), "{name} is not a directory");
    Ok(path)
}

fn validate_current_worktree(worktree: &Path) -> Result<()> {
    if let Some(raw) = std::env::var_os("LOOM_WORKTREE_PATH") {
        ensure!(
            fs::canonicalize(raw)? == worktree,
            "LOOM_WORKTREE_PATH differs from session worktree"
        );
    }
    let cwd = fs::canonicalize(std::env::current_dir()?)?;
    ensure!(
        cwd.starts_with(worktree),
        "current directory is outside the owned worktree"
    );
    Ok(())
}

fn common_parent(
    scope: &IdentityScope,
    specs: &[WorkerSpec],
    requested: Option<&str>,
    starts: &StartedAgentTypeIndex,
    authorizations: &[CodexAuthorization],
) -> Result<String> {
    if let Some(parent) = requested {
        validate_parent_uuid(parent)?;
        ensure!(
            specs
                .iter()
                .all(|spec| can_bind(scope, spec, parent, starts, authorizations)),
            "requested Claude parent does not own every worker"
        );
        return Ok(parent.to_owned());
    }
    let mut candidates: Option<BTreeSet<String>> = None;
    for spec in specs {
        let next = candidate_parents(scope, spec, starts, authorizations)?;
        candidates = Some(match candidates {
            None => next,
            Some(known) => known.intersection(&next).cloned().collect(),
        });
    }
    let candidates = candidates.unwrap_or_default();
    ensure!(
        candidates.len() == 1,
        "worker set does not resolve to one Claude parent UUID"
    );
    candidates.into_iter().next().context("missing parent")
}

fn candidate_parents(
    scope: &IdentityScope,
    spec: &WorkerSpec,
    starts: &StartedAgentTypeIndex,
    authorizations: &[CodexAuthorization],
) -> Result<BTreeSet<String>> {
    match spec.kind {
        WorkerKind::Claude => claude_parent_candidates(scope, &spec.id, starts),
        WorkerKind::Codex => Ok(authorizations
            .iter()
            .filter(|row| authorization_matches(scope, row, &spec.id, None))
            .map(|row| row.parent_session_id.clone())
            .filter(|parent| validate_parent_uuid(parent).is_ok())
            .collect()),
    }
}

fn claude_parent_candidates(
    scope: &IdentityScope,
    agent_id: &str,
    starts: &StartedAgentTypeIndex,
) -> Result<BTreeSet<String>> {
    let project = scope
        .projects_root
        .join(project_slug(&scope.canonical_worktree));
    let entries = match fs::read_dir(project) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error.into()),
    };
    let mut parents = BTreeSet::new();
    for entry in entries.take(MAX_PARENT_DIRS) {
        let entry = entry?;
        let Some(parent) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if entry.file_type()?.is_dir()
            && validate_parent_uuid(&parent).is_ok()
            && starts
                .resolve_exact(&scope.stage_id, &parent, &scope.loom_session_id, agent_id)
                .is_some()
        {
            parents.insert(parent);
        }
    }
    Ok(parents)
}

fn can_bind(
    scope: &IdentityScope,
    spec: &WorkerSpec,
    parent: &str,
    starts: &StartedAgentTypeIndex,
    authorizations: &[CodexAuthorization],
) -> bool {
    match spec.kind {
        WorkerKind::Claude => starts
            .resolve_exact(&scope.stage_id, parent, &scope.loom_session_id, &spec.id)
            .is_some(),
        WorkerKind::Codex => {
            authorizations
                .iter()
                .filter(|row| authorization_matches(scope, row, &spec.id, Some(parent)))
                .count()
                == 1
        }
    }
}

fn bind_worker(
    scope: &IdentityScope,
    spec: WorkerSpec,
    parent: &str,
    starts: &StartedAgentTypeIndex,
    authorizations: &[CodexAuthorization],
) -> Result<BoundWorker> {
    match spec.kind {
        WorkerKind::Claude => bind_claude(scope, spec, parent, starts),
        WorkerKind::Codex => bind_codex(scope, spec, parent, authorizations),
    }
}

fn bind_claude(
    scope: &IdentityScope,
    spec: WorkerSpec,
    parent: &str,
    starts: &StartedAgentTypeIndex,
) -> Result<BoundWorker> {
    let start = starts
        .resolve_exact(&scope.stage_id, parent, &scope.loom_session_id, &spec.id)
        .context("missing or ambiguous exact SubagentStart row")?;
    let subagents_dir = scope
        .projects_root
        .join(project_slug(&scope.canonical_worktree))
        .join(parent)
        .join("subagents");
    let transcript = resolve_claude_transcript(&subagents_dir, &spec.id)?;
    let identity = WorkerIdentity::ClaudeSubagent {
        stage_id: scope.stage_id.clone(),
        loom_session_id: scope.loom_session_id.clone(),
        parent_session_id: parent.to_owned(),
        agent_id: spec.id.clone(),
        agent_type: start.agent_type,
        transcript_path: transcript.clone(),
    };
    Ok(BoundWorker {
        worker: spec.clone(),
        lifecycle_identity: identity,
        authority: None,
        evidence: vec![
            EvidenceReference {
                source: "claude_start".into(),
                path: scope
                    .work_dir
                    .join("subagents")
                    .join(&scope.stage_id)
                    .join("starts.jsonl"),
                identity: format!("{}:{}", parent, spec.id),
            },
            EvidenceReference {
                source: "claude_transcript".into(),
                path: transcript,
                identity: spec.id,
            },
        ],
    })
}

fn bind_codex(
    scope: &IdentityScope,
    spec: WorkerSpec,
    parent: &str,
    authorizations: &[CodexAuthorization],
) -> Result<BoundWorker> {
    let matches: Vec<_> = authorizations
        .iter()
        .filter(|row| authorization_matches(scope, row, &spec.id, Some(parent)))
        .cloned()
        .collect();
    ensure!(
        matches.len() == 1,
        "missing or ambiguous Codex authorization"
    );
    let authorization = matches
        .into_iter()
        .next()
        .context("missing authorization")?;
    ensure!(
        authorization.workspace_root == scope.canonical_worktree,
        "Codex authorization names another worktree"
    );
    let execution = super::codex_binding::resolve_wait_codex_execution(
        &scope.work_dir,
        &scope.stage_id,
        &authorization,
    )?;
    Ok(BoundWorker {
        worker: spec,
        lifecycle_identity: execution.identity,
        authority: Some(CodexAuthority::from(authorization.clone())),
        evidence: vec![
            EvidenceReference {
                source: "codex_authorization".into(),
                path: codex_path(scope),
                identity: authorization.invocation_id,
            },
            EvidenceReference {
                source: execution.source,
                path: execution.path,
                identity: execution.label,
            },
        ],
    })
}

fn authorization_matches(
    scope: &IdentityScope,
    row: &CodexAuthorization,
    unit: &str,
    parent: Option<&str>,
) -> bool {
    row.stage_id == scope.stage_id
        && row.loom_session_id == scope.loom_session_id
        && row.unit_id == unit
        && parent.is_none_or(|value| row.parent_session_id == value)
}

fn codex_path(scope: &IdentityScope) -> PathBuf {
    scope
        .work_dir
        .join("subagents")
        .join(&scope.stage_id)
        .join("codex.jsonl")
}

fn validate_parent_uuid(parent: &str) -> Result<()> {
    let parsed = Uuid::parse_str(parent).context("Claude parent must be a UUID")?;
    ensure!(
        parsed.to_string() == parent.to_ascii_lowercase(),
        "Claude parent UUID is not canonical"
    );
    Ok(())
}
