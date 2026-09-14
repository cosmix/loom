//! Persistence and trust checks for completion evidence.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::daemon::protocol::Response;
use crate::fs::locking::locked_dir_update;
use crate::fs::session_files::validate_session_file_id;
use crate::fs::work_dir::WorkDir;
use crate::handoff::generator::load_trusted_session_checkpoint;
use crate::handoff::{
    check_definition_hash, expected_stage_commit, record_accepted_handoff, record_attempt_handoff,
    AcceptedReceipt, CompletionAttemptEvidence, CompletionPhase,
};
use crate::models::stage::Stage;
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::load_stage;

pub(crate) fn refusal_response(error: anyhow::Error) -> Response {
    const MAX_CHARS: usize = 1024;
    let message = format!("Completion evidence refused: {error:#}")
        .chars()
        .take(MAX_CHARS)
        .collect();
    Response::Error { message }
}

pub(crate) fn handle_record(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    evidence: CompletionAttemptEvidence,
) -> Result<Response> {
    evidence.validate().context("invalid completion evidence")?;
    if evidence.stage_id != stage_id {
        bail!("completion evidence stage binding failed");
    }
    if evidence.session_id != session_id {
        bail!("completion evidence session binding failed");
    }
    let sessions_dir = sessions_dir(work_dir)?;
    locked_dir_update(&sessions_dir, || {
        super::control_complete::validate_active_identity(work_dir, stage_id, session_id)?;
        let stage = load_stage(stage_id, work_dir)?;
        let session = exact_session(work_dir, session_id)?;
        record_attempt_handoff(&session, &stage, &evidence, work_dir)?;
        Ok(())
    })?;
    Ok(Response::Ok)
}

pub(crate) fn verified_commit(
    work_dir: &Path,
    stage: &Stage,
    session_id: &str,
    evidence_nonce: &str,
) -> Result<String> {
    let repo_root = repo_root(work_dir)?;
    Ok(
        require_verified_checkpoint(work_dir, &repo_root, stage, session_id, evidence_nonce)?
            .commit,
    )
}

fn require_verified_checkpoint(
    work_dir: &Path,
    repo_root: &Path,
    stage: &Stage,
    session_id: &str,
    evidence_nonce: &str,
) -> Result<CompletionAttemptEvidence> {
    let checkpoint = load_trusted_session_checkpoint(&stage.id, session_id, work_dir)
        .context("completion checkpoint read binding failed")?
        .context("completion checkpoint binding is missing")?;
    if checkpoint.stage_id != stage.id || checkpoint.session_id != session_id {
        bail!("completion checkpoint identity binding failed");
    }
    if checkpoint.conflict {
        bail!("completion checkpoint conflict binding failed");
    }
    if checkpoint.accepted.is_some() {
        bail!("completion checkpoint accepted-receipt binding failed");
    }
    let evidence = checkpoint
        .latest
        .context("completion checkpoint latest-attempt binding is missing")?;
    verify_evidence_bindings(&evidence, stage, repo_root, session_id, evidence_nonce)?;
    Ok(evidence)
}

pub(crate) fn persist_acceptance_receipt(
    work_dir: &Path,
    stage: &Stage,
    session_id: &str,
    evidence_nonce: &str,
    completion_nonce: &str,
    commit: String,
) -> Result<()> {
    let receipt = AcceptedReceipt {
        evidence_nonce: evidence_nonce.to_string(),
        completion_nonce: completion_nonce.to_string(),
        commit,
        attestation: None,
    };
    let session = exact_session(work_dir, session_id)?;
    record_accepted_handoff(&session, stage, receipt, work_dir)?;
    Ok(())
}

/// Relies on the daemon work directory being main-checkout state: a worktree-rooted path would
/// make `WorkDir::project_root` bind `expected_stage_commit` to the wrong knowledge repository.
fn repo_root(work_dir: &Path) -> Result<PathBuf> {
    WorkDir::new(work_dir)
        .context("completion repository binding failed")?
        .project_root()
        .map(Path::to_path_buf)
        .context("completion repository binding is unavailable")
}

fn verify_evidence_bindings(
    evidence: &CompletionAttemptEvidence,
    stage: &Stage,
    repo_root: &Path,
    session_id: &str,
    evidence_nonce: &str,
) -> Result<()> {
    if evidence.stage_id != stage.id || evidence.session_id != session_id {
        bail!("completion checkpoint identity binding failed");
    }
    if evidence.evidence_nonce != evidence_nonce {
        bail!("completion checkpoint evidence-nonce binding failed");
    }
    if evidence.phase != CompletionPhase::VerifiedPendingAck {
        bail!("completion checkpoint phase binding failed");
    }
    if !evidence.verification.all_passed() {
        bail!("completion checkpoint verification binding failed");
    }
    if evidence.check_definition_hash != check_definition_hash(stage) {
        bail!("completion checkpoint check-definition binding failed");
    }
    let expected = expected_stage_commit(stage, repo_root)
        .context("completion checkpoint commit binding failed")?;
    if evidence.commit != expected {
        bail!("completion checkpoint commit binding failed");
    }
    Ok(())
}

fn exact_session(work_dir: &Path, session_id: &str) -> Result<crate::models::session::Session> {
    const MAX_SESSION_BYTES: usize = 1024 * 1024;
    validate_session_file_id(session_id).context("invalid completion session-record binding")?;
    let relative = PathBuf::from("sessions").join(format!("{session_id}.md"));
    let content =
        crate::fs::safe_read::read_to_string_bounded(work_dir, &relative, MAX_SESSION_BYTES)
            .context("completion checkpoint session-record binding is unreadable")?;
    let session: crate::models::session::Session =
        parse_from_markdown(&content, "session").context("invalid completion session record")?;
    if session.id != session_id {
        bail!("completion checkpoint session-record identity binding failed");
    }
    Ok(session)
}

fn sessions_dir(work_dir: &Path) -> Result<PathBuf> {
    let path = work_dir.join("sessions");
    if !path.is_dir() {
        bail!("completion sessions directory is unavailable");
    }
    Ok(path)
}

#[cfg(test)]
pub(crate) struct TrustedCheckpointFixture {
    _scratch: ScratchGitFixture,
    pub(crate) work: PathBuf,
    pub(crate) stage: Stage,
    pub(crate) session: crate::models::session::Session,
}

#[cfg(test)]
impl TrustedCheckpointFixture {
    pub(crate) fn complete(&self, nonce: &str, evidence_nonce: &str) -> Result<Response> {
        super::control_complete::handle_complete_stage(
            &self.work,
            &self.stage.id,
            &self.session.id,
            nonce,
            evidence_nonce,
        )
    }
}

#[cfg(test)]
pub(crate) fn trusted_checkpoint_fixture(
    stage_id: &str,
    stage_type: crate::models::stage::StageType,
    session_type: crate::models::session::SessionType,
    evidence_nonce: &str,
) -> TrustedCheckpointFixture {
    use crate::models::stage::AcceptanceCriterion;
    use crate::verify::transitions::save_stage;

    let scratch = scratch_git_fixture();
    let repo = &scratch.repo;
    let work = &scratch.work;
    let mut session = crate::models::session::Session::new();
    session.session_type = session_type;
    session.assign_to_stage(stage_id.to_string());
    session.status = crate::models::session::SessionStatus::Running;
    let mut stage = Stage::new(stage_id.to_string(), None);
    stage.id = stage_id.to_string();
    stage.stage_type = stage_type;
    stage.status = crate::models::stage::StageStatus::Executing;
    stage.session = Some(session.id.clone());
    stage.acceptance = vec![AcceptanceCriterion::Simple("true".to_string())];
    if stage_type != crate::models::stage::StageType::Knowledge {
        let branch = crate::git::branch::branch_name_for_stage(stage_id);
        let _ = scratch_git(repo, &["branch", &branch]);
    }
    save_stage(&stage, work).unwrap();
    crate::fs::session_files::save_session(&session, work).unwrap();
    record_trusted_test_checkpoint(repo, work, &stage, &session, evidence_nonce);
    TrustedCheckpointFixture {
        work: work.to_path_buf(),
        stage,
        session,
        _scratch: scratch,
    }
}

#[cfg(test)]
pub(crate) struct ScratchGitFixture {
    _temp: tempfile::TempDir,
    pub(crate) repo: PathBuf,
    pub(crate) work: PathBuf,
    pub(crate) commit: String,
}

#[cfg(test)]
pub(crate) fn scratch_git_fixture() -> ScratchGitFixture {
    let temp = tempfile::TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let _ = scratch_git(&repo, &["init", "-q"]);
    let _ = scratch_git(
        &repo,
        &[
            "-c",
            "user.name=Loom Test",
            "-c",
            "user.email=loom@test.invalid",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "initial",
        ],
    );
    let commit = scratch_git(&repo, &["rev-parse", "HEAD"]);
    let work = repo.join(".loom").join("work");
    for dir in ["sessions", "stages", "handoffs", "signals"] {
        std::fs::create_dir_all(work.join(dir)).unwrap();
    }
    ScratchGitFixture {
        _temp: temp,
        repo,
        work,
        commit,
    }
}

#[cfg(test)]
fn record_trusted_test_checkpoint(
    repo: &Path,
    work: &Path,
    stage: &Stage,
    session: &crate::models::session::Session,
    evidence_nonce: &str,
) {
    use crate::handoff::{CriterionResult, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION};

    let evidence = CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        commit: expected_stage_commit(stage, repo).unwrap(),
        check_definition_hash: check_definition_hash(stage),
        exact_command: format!("loom check {}", stage.id),
        evidence_nonce: evidence_nonce.to_string(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "acceptance-0".to_string(),
                passed: true,
            }],
            environment_policy: crate::handoff::STAGE_ENVIRONMENT_POLICY.to_string(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: None,
        diagnostic_first_line: None,
        observed_at: "2026-09-14T10:00:00Z".to_string(),
        attestation: None,
    };
    record_attempt_handoff(session, stage, &evidence, work).unwrap();
}

#[cfg(test)]
pub(crate) fn scratch_git(repo: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", repo.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", repo.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
