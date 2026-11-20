//! What one `loom check` runs (`CheckArgs` to `CheckSelection`), and the
//! contracts section a full or `--contracts` run prints.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use colored::Colorize;

use super::normalized_criteria;
use crate::commands::stage::acceptance_runner::StageExecutionPaths;
use crate::commands::stage::parse_replacements;
use crate::models::stage::Stage;
use crate::verify::contracts::completion::{self, ContractCheck};

/// Why the operator's shell does not run a replacement trial.
pub(super) const TRIAL_NEEDS_SESSION: &str =
    "a replacement trial runs as a host job, which only the stage's session can request: run \
     `loom check <stage> --contracts --replacement <frozen-file>=<path>` there";

/// `loom check`'s run options.
#[derive(Debug, Clone, clap::Args)]
pub struct CheckArgs {
    /// Skip the acceptance-criterion pass cache lookup: run every criterion
    /// for real even if an identical tree already passed it. Passes are
    /// still stored, so a later `loom stage complete` can reuse them.
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Run only the acceptance criterion at this 0-based index, the number in
    /// its `[criterion <n>]` label (repeatable). A subset run skips the
    /// goal-backward checks.
    #[arg(long = "criterion", value_name = "INDEX")]
    pub criterion: Vec<usize>,

    /// Run only the stage's frozen contracts, as the completion check runs
    /// them, instead of its acceptance criteria.
    #[arg(long = "contracts")]
    pub contracts: bool,

    /// With --contracts: trial a replacement for a frozen file, laid over it
    /// for the run only (repeatable). A contract dispute carrying the same
    /// replacement needs a passing trial first.
    #[arg(long = "replacement", value_name = "FROZEN_FILE=PATH")]
    pub replacement: Vec<String>,
}

/// What one `loom check` runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckSelection {
    /// Every acceptance criterion, the goal-backward checks and, on a
    /// `standard` stage whose contracts are frozen, the contracts.
    All,
    /// Only these 0-based acceptance criteria, sorted and deduplicated.
    Criteria(Vec<usize>),
    /// Only the frozen contracts.
    Contracts,
    /// The frozen contracts with each `(frozen file, replacement path)` laid
    /// over its frozen file.
    Trial(Vec<(String, PathBuf)>),
}

/// The selection `args` names. `--contracts` with `--criterion`,
/// `--replacement` without `--contracts`, and a malformed or repeated
/// replacement are refused, so no flag is silently dropped.
pub fn check_selection(args: &CheckArgs) -> Result<CheckSelection> {
    if !args.contracts {
        if !args.replacement.is_empty() {
            bail!("--replacement trials a replacement of a frozen file and needs --contracts");
        }
        let criteria = normalized_criteria(&args.criterion);
        return Ok(if criteria.is_empty() {
            CheckSelection::All
        } else {
            CheckSelection::Criteria(criteria)
        });
    }
    if !args.criterion.is_empty() {
        bail!("--contracts runs the frozen contracts, not acceptance criteria: drop --criterion");
    }
    if args.replacement.is_empty() {
        return Ok(CheckSelection::Contracts);
    }
    Ok(CheckSelection::Trial(parse_replacements(
        &args.replacement,
    )?))
}

/// Run and print the frozen contracts, as the completion check runs them.
pub(super) fn run_contracts_section(
    stage: &Stage,
    execution_paths: &StageExecutionPaths,
    work_dir: &Path,
) -> Result<ContractCheck> {
    println!("\n{}", "Contracts:".bold());
    let worktree_root = execution_paths
        .worktree_root
        .as_deref()
        .context("No worktree available to run the contracts in")?;
    let acceptance_dir = execution_paths
        .acceptance_dir
        .as_deref()
        .unwrap_or(worktree_root);
    let check = completion::run(stage, work_dir, acceptance_dir, worktree_root, &[])?;
    for drift in &check.drift {
        println!("  {} {drift}", "✗".red());
    }
    for contract in &check.contracts {
        let mark = if contract.failures.is_empty() {
            "✓".green()
        } else {
            "✗".red()
        };
        println!("  {mark} contract:{}", contract.id);
    }
    Ok(check)
}
