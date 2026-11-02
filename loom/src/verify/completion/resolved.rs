//! Completion checks for a trusted resolver result before merge landing.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::{run_checks, OutputStream, VerificationChecks};
use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::stage::{Stage, StageStatus};
use crate::utils::truncate_for_display;
use crate::verify::criteria::output::format_criterion_result;
use crate::verify::criteria::{
    plan_confinement, run_acceptance_with_config, CriteriaConfig, CriterionResult,
};
use crate::verify::execution_paths::resolve_acceptance_dir;
use crate::verify::transitions::update_stage;

/// Verify a resolver's result for a landing the daemon decides alone: a wiring
/// or duplicate detector that cannot run fails the completion, as it does for
/// a daemon completion job, since nobody reads a warning here.
pub(crate) fn verify_resolved_completion(
    stage: &Stage,
    work_dir: &Path,
    emit: fn(OutputStream, &str),
) -> Result<Stage> {
    verify_resolved(stage, work_dir, emit, true)
}

/// [`verify_resolved_completion`] for an operator at a terminal, who reads the
/// warning a detector that cannot run leaves and decides for themselves.
pub(crate) fn verify_resolved_completion_for_operator(
    stage: &Stage,
    work_dir: &Path,
    emit: fn(OutputStream, &str),
) -> Result<Stage> {
    verify_resolved(stage, work_dir, emit, false)
}

/// Verify the resolved worktree and preserve the original ancestry proof.
fn verify_resolved(
    stage: &Stage,
    work_dir: &Path,
    emit: fn(OutputStream, &str),
    fail_closed: bool,
) -> Result<Stage> {
    let repo = crate::fs::work_dir::WorkDir::new(work_dir)?
        .main_project_root()
        .context("completion repository is unavailable")?;
    crate::git::merge::check_resolved_worktree(&repo, &stage.id, stage.completed_commit.as_deref())
        .map_err(anyhow::Error::msg)?;
    let root = crate::git::worktree::stage_worktree_path(stage, &repo)?;
    let acceptance_dir = resolve_acceptance_dir(Some(&root), stage.working_dir.as_deref())?;
    let snapshot = VerificationSnapshot::capture(stage, work_dir)?;
    run_acceptance(stage, work_dir, &root, acceptance_dir.as_deref(), emit)?;
    run_checks(
        &VerificationChecks {
            stage,
            stage_id: &stage.id,
            acceptance_dir: acceptance_dir.as_deref(),
            worktree_root: Some(&root),
            daemon_gates: false,
            fail_closed,
            expected_policy: None,
            emit,
            work_dir,
        },
        &mut |_, checked| checked,
    )?;
    update_stage(&stage.id, work_dir, |current| {
        anyhow::ensure!(
            matches!(
                current.status,
                StageStatus::MergeConflict | StageStatus::MergeBlocked
            ),
            "stage left merge resolution during verification"
        );
        snapshot.validate_locked(current, work_dir)?;
        current.completed_commit = Some(snapshot.commit.clone());
        Ok(())
    })
}

fn run_acceptance(
    stage: &Stage,
    work_dir: &Path,
    root: &Path,
    acceptance_dir: Option<&Path>,
    emit: fn(OutputStream, &str),
) -> Result<()> {
    if stage.acceptance.is_empty() {
        return Ok(());
    }
    emit(
        OutputStream::Stdout,
        &format!("Running acceptance criteria for stage '{}'...", stage.id),
    );
    if let Some(dir) = acceptance_dir {
        emit(
            OutputStream::Stdout,
            &format!("  (working directory: {})", dir.display()),
        );
    }
    let config = CriteriaConfig::default()
        .with_plan_confinement(plan_confinement(work_dir))
        .with_cache_dir(work_dir)
        .with_tree_root(Some(root));
    let result = run_acceptance_with_config(stage, acceptance_dir, &config)
        .context("Failed to run acceptance criteria")?;
    for (index, criterion) in result.results().iter().enumerate() {
        emit(
            OutputStream::Stdout,
            &format_criterion_result(index, criterion, &stage.setup),
        );
    }
    if !result.all_passed() {
        emit(
            OutputStream::Stderr,
            &format!("Acceptance criteria FAILED for stage '{}'", stage.id),
        );
        bail!(
            "Acceptance criteria failed for stage '{}': {}",
            stage.id,
            failed_commands(result.results())
        );
    }
    emit(OutputStream::Stdout, "All acceptance criteria passed!");
    Ok(())
}

/// The failing commands on one line, for the refusal a resolver reads. The
/// bounds leave room in the refusal for the rest of its text.
fn failed_commands(results: &[CriterionResult]) -> String {
    const COMMAND_MAX_CHARS: usize = 200;
    const LIST_MAX_CHARS: usize = 1024;
    let commands = results
        .iter()
        .filter(|criterion| !criterion.passed())
        .map(|criterion| truncate_for_display(&criterion.command, COMMAND_MAX_CHARS))
        .collect::<Vec<_>>()
        .join("; ");
    truncate_for_display(&commands, LIST_MAX_CHARS)
}

#[cfg(test)]
#[path = "resolved_tests.rs"]
mod tests;
