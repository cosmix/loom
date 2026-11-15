//! Acceptance criteria directory resolution
//!
//! This module provides helpers for resolving the working directory
//! where acceptance criteria should be executed.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::Path;

use super::criterion_output::format_criterion_result;
use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::stage::Stage;
use crate::verify::criteria::{
    plan_confinement, run_acceptance_with_config, CriteriaConfig, CriterionResult,
};
pub(crate) use crate::verify::execution_paths::{resolve_acceptance_dir, StageExecutionPaths};

/// Display behavior for acceptance command output.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AcceptanceDisplayOptions<'a> {
    /// Optional label used in the start line (e.g. "stage", "knowledge stage").
    pub stage_label: Option<&'a str>,
    /// Whether to print an explicit message when no acceptance criteria are defined.
    pub show_empty_message: bool,
}

/// Resolve both worktree root and acceptance directory for a standard stage.
pub(crate) fn resolve_stage_execution_paths(stage: &Stage) -> Result<StageExecutionPaths> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd)
        .context("Cannot resolve the repository for stage acceptance")?;
    let assigned = stage
        .worktree
        .as_deref()
        .context("Stage has no assigned worktree; recover the stage before verification")?;
    crate::validation::validate_id(assigned)?;
    let root = repo_root.join(".worktrees").join(assigned);
    anyhow::ensure!(
        root.is_dir(),
        "Assigned worktree '{}' is missing; recover the stage worktree before verification",
        root.display()
    );
    let git = crate::git::worktree::WorktreeGit::pinned(&repo_root, &root)?;
    verify_stage_branch(stage, &git)?;
    let worktree_root = Some(root);

    let acceptance_dir =
        resolve_acceptance_dir(worktree_root.as_deref(), stage.working_dir.as_deref())?;

    Ok(StageExecutionPaths {
        worktree_root,
        acceptance_dir,
    })
}

fn verify_stage_branch(stage: &Stage, git: &crate::git::worktree::WorktreeGit) -> Result<()> {
    let branch = crate::git::branch::checked_out_branch_with(git)?;
    anyhow::ensure!(
        branch == Some(crate::git::branch::branch_name_for_stage(&stage.id)),
        "Assigned worktree does not belong to stage '{}'",
        stage.id
    );
    Ok(())
}

/// Resolve execution paths for knowledge stages in main repository context.
/// The tree is the acceptance directory itself (no `worktree_root`), so a
/// `working_dir` below the repository root does not grant `.git`, `.loom` or
/// `.worktrees` read-write.
pub(crate) fn resolve_knowledge_execution_paths(stage: &Stage) -> Result<StageExecutionPaths> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or(cwd);
    let acceptance_dir =
        resolve_acceptance_dir(Some(repo_root.as_path()), stage.working_dir.as_deref())?;
    Ok(StageExecutionPaths {
        worktree_root: None,
        acceptance_dir,
    })
}

/// Run acceptance criteria and print standardized output.
///
/// Criteria start in `paths.acceptance_dir` and may use all of
/// `paths.worktree_root`, so a stage whose `working_dir` is one package of a
/// monorepo still reaches its sibling packages and root-level stores.
///
/// `work_dir` is the state directory, used to pick up the plan-level
/// command confinement default; the stage's own override still wins over it.
///
/// Returns `true` when all criteria pass, `false` otherwise.
pub(crate) fn run_acceptance_with_display(
    stage: &Stage,
    stage_id: &str,
    paths: &StageExecutionPaths,
    work_dir: &Path,
    options: AcceptanceDisplayOptions<'_>,
) -> Result<bool> {
    run_selected_acceptance(stage, stage_id, paths, work_dir, options, None)
}

/// [`run_acceptance_with_display`] limited to the 0-based criteria in `only`
/// (`loom check --criterion`); `None` runs every criterion.
pub(crate) fn run_selected_acceptance(
    stage: &Stage,
    stage_id: &str,
    paths: &StageExecutionPaths,
    work_dir: &Path,
    options: AcceptanceDisplayOptions<'_>,
    only: Option<&BTreeSet<usize>>,
) -> Result<bool> {
    let acceptance_dir = paths.acceptance_dir.as_deref();
    if stage.acceptance.is_empty() && only.is_none() {
        if options.show_empty_message {
            println!("No acceptance criteria defined, treating as passed.");
        }
        return Ok(true);
    }

    if let Some(label) = options.stage_label {
        println!("Running acceptance criteria for {label} '{stage_id}'...");
    }
    if let Some(dir) = acceptance_dir {
        println!("  (working directory: {})", dir.display());
    }

    let config = CriteriaConfig::default()
        .with_plan_confinement(plan_confinement(work_dir))
        .with_cache_dir(work_dir)
        .with_tree_root(paths.worktree_root.as_deref())
        .with_only(only.cloned());
    let result = run_acceptance_with_config(stage, acceptance_dir, &config)
        .context("Failed to run acceptance criteria")?;

    // run_acceptance_with_config pushes one CriterionResult per selected
    // criterion, in list order, so each result pairs with its index in
    // `stage.acceptance`: the index `loom stage dispute-criteria
    // --criterion-index` and `loom check --criterion` take.
    let indices = (0..stage.acceptance.len()).filter(|index| config.selects(*index));
    for (index, criterion_result) in indices.zip(result.results()) {
        print_criterion_result(index, criterion_result, &stage.setup);
    }

    if result.all_passed() {
        let which = if only.is_some() { "Selected" } else { "All" };
        println!("{which} acceptance criteria passed!");
    }

    Ok(result.all_passed())
}

/// Print one criterion's pass/fail/timeout line. `index` is the same index
/// `loom stage dispute-criteria --criterion-index` takes; `setup` is the
/// stage's setup commands, shown (with the exit code and output tails) only
/// when the criterion failed or timed out — see `criterion_output`.
fn print_criterion_result(index: usize, criterion_result: &CriterionResult, setup: &[String]) {
    println!(
        "{}",
        format_criterion_result(index, criterion_result, setup)
    );
}

/// The guidance for a failed acceptance, wiring or wiring-test check: fix it,
/// or dispute a check no correct work could pass or that fails only because of
/// where completion runs. Block is refused while the latest completion shows a failing criterion.
fn failure_guidance(stage_id: &str) -> String {
    let dispute = format!("    loom stage dispute-criteria {stage_id}");
    [
        format!("  Fix the issues and run 'loom stage complete {stage_id}' again\n"),
        "  A check no correct implementation could pass gets a dispute, never a weakened fix. \
         Commit first; filing ends this session by design:"
            .to_string(),
        format!("{dispute} --criterion-index <n> --reason \"<why>\"  ([criterion n])"),
        format!("{dispute} --field wiring --criterion-index <n> --reason \"<why>\"  ([wiring n])"),
        format!(
            "{dispute} --field wiring-tests --criterion-index <n> --reason \"<why>\"  \
             ([wiring_tests n])"
        ),
        "  <n> is the number in the label above; --failure-output <path> ships the captured \
         output as evidence. This routes to the adjudicator, which can amend the check. Do \
         not reach for --no-verify — it needs an operator proof the sandbox denies by \
         design — and do not corrupt the artifact to satisfy a check you believe is wrong."
            .to_string(),
        "  A criterion that fails only because of where completion runs (no network, \
         unreadable git metadata, the command timeout, a sandbox denial) is disputed, never \
         blocked: name every such criterion in one filing with a repeatable \
         --criterion-index <n> flag. loom stage block is for a credential, a host install or \
         a decision only a person can make, and loom refuses it while the latest \
         `loom stage complete` shows a failing criterion."
            .to_string(),
    ]
    .join("\n")
}

/// Print what to do about a failed acceptance, wiring or wiring-test check.
pub(crate) fn print_acceptance_failure_guidance(stage_id: &str) {
    eprintln!("{}", failure_guidance(stage_id));
}

#[cfg(test)]
#[path = "acceptance_runner_tests.rs"]
mod tests;

#[cfg(test)]
mod branch_collision_tests {
    use super::*;

    #[test]
    fn stage_branch_with_same_named_tag_is_accepted() {
        let repo = tempfile::tempdir().unwrap();
        for args in [
            vec!["init", "-q", "-b", "loom/s1"],
            vec![
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@t",
                "commit",
                "--allow-empty",
                "-qm",
                "seed",
            ],
            vec!["tag", "loom/s1"],
        ] {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(repo.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        let stage = Stage {
            id: "s1".into(),
            ..Default::default()
        };
        let git = crate::git::worktree::WorktreeGit::discovered(repo.path());
        verify_stage_branch(&stage, &git).unwrap();
    }
}
