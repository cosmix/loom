//! Shared contract freeze preparation, validation, and judgment.

use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use crate::daemon::ContractRunReport;
use crate::git::worktree::WorktreeGit;
use crate::models::stage::CommandConfinement;
use crate::plan::schema::ContractSpec;
use crate::testrun::{classify, RunOutcome, RunSummary, TestRunnerAdapter};
use crate::verify::contracts::changes::{changed_paths, harness_files, stage_base};
use crate::verify::contracts::collection::{
    freeze_scaffold, stage_scaffold, ScaffoldCheck, ScaffoldEdit,
};
use crate::verify::contracts::format_gate::{format_problems, static_problems};
use crate::verify::contracts::site::ContractSite;
use crate::verify::contracts::{is_contract_or_harness, normalize};

/// How long one contract test may run.
const CONTRACT_TIMEOUT: Duration = Duration::from_secs(300);

/// The report outcome of a run whose build failed.
pub(crate) const BUILD_FAILED: &str = "build_failed";

/// Outcome of a run whose output said nothing readable but whose exit was not 0.
pub(crate) const UNVERIFIED: &str = "exit_nonzero_unverified";

/// What the contract session changed and which of its files exist; every
/// path is relative to the stage's `working_dir`.
pub(crate) struct FreezeInputs<'a> {
    pub contracts: &'a [ContractSpec],
    pub harness: &'a [String],
    pub changed_paths: &'a [String],
    pub existing: &'a dyn Fn(&str) -> bool,
    /// The scaffold edits the contracts' collection needs, and why the
    /// worktree's version of them (or the contracts' module chains) is wrong.
    pub scaffold: &'a ScaffoldCheck,
}

/// D8 steps 1 and 2: every changed path is a contract file, matches a
/// `harness` glob or is a scaffold edit, the scaffold edits and module chains
/// are sound, and every contract file exists.
pub(crate) fn check_changes(inputs: &FreezeInputs<'_>) -> Result<(), Vec<String>> {
    let mut problems: Vec<String> = inputs
        .changed_paths
        .iter()
        .filter(|path| {
            !is_contract_or_harness(path.as_str(), inputs.contracts, inputs.harness)
                && !inputs.scaffold.paths.contains(path.as_str())
        })
        .map(|path| {
            format!(
                "{path} is neither a contract file, matched by a `harness` glob, \
                 nor a scaffold edit"
            )
        })
        .collect();
    problems.extend(inputs.scaffold.problems.iter().cloned());
    problems.extend(
        inputs
            .contracts
            .iter()
            .filter(|contract| !(inputs.existing)(&contract.file))
            .map(|contract| {
                format!(
                    "contract `{}`: {} does not exist",
                    contract.id, contract.file
                )
            }),
    );
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// D8 step 3 for one run: `Failed` or `BuildFailed` is required. An adapter
/// that could not read the output, or no adapter at all, leaves only the
/// exit code, which must then be non-zero.
pub(crate) fn judge_run(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    summary: RunSummary,
    exit: Option<i32>,
) -> Result<ContractRunReport, String> {
    let id = &contract.id;
    let outcome = match adapter.map(|_| classify(&summary, exit)) {
        Some(RunOutcome::Failed) => "failed",
        Some(RunOutcome::BuildFailed) => BUILD_FAILED,
        Some(RunOutcome::Passed) => {
            return Err(format!(
                "contract `{id}` passes before implementation, so it cannot tell right from \
                 wrong: make {} assert behaviour the stage has not built yet",
                contract.file
            ))
        }
        Some(RunOutcome::NotSelected) => {
            return Err(format!(
                "contract `{id}`: the runner did not select the test `{}` in {}; its name \
                 must be exactly the one the runner filters by",
                contract.test.as_deref().unwrap_or(""),
                contract.file
            ))
        }
        Some(RunOutcome::Unparsed) | None if exit != Some(0) => UNVERIFIED,
        Some(RunOutcome::Unparsed) | None => {
            return Err(format!(
                "contract `{id}` exited 0 and its output showed no failing test, so nothing \
                 shows it fails"
            ))
        }
    };
    Ok(ContractRunReport {
        contract_id: id.clone(),
        adapter: adapter.map(|adapter| adapter.name().to_string()),
        outcome: outcome.to_string(),
        exit_code: exit,
    })
}

/// D8 steps 1 and 2 with `repo`: the stage declares contracts, every changed
/// path is a contract file or matches a `harness` glob, and every contract
/// file exists. A failure is a refusal listing the problems.
pub(crate) fn check_site_changes(site: &ContractSite, repo: &WorktreeGit) -> Result<()> {
    let stage = &site.stage;
    if stage.contracts.is_empty() {
        bail!(
            "Stage '{}' declares no contracts, so there is nothing to freeze",
            stage.id
        );
    }
    let base = stage_base(repo, &site.work_dir)?;
    let changed = changed_paths(repo, &site.working_dir, &base)?;
    let existing = |file: &str| site.working_dir.join(file).is_file();
    let scaffold = freeze_scaffold(
        repo,
        &site.worktree_root,
        &site.working_dir,
        &base,
        &stage.contracts,
    )?;
    let inputs = FreezeInputs {
        contracts: &stage.contracts,
        harness: &stage.harness,
        changed_paths: &changed,
        existing: &existing,
        scaffold: &scaffold,
    };
    refuse(
        &stage.id,
        check_changes(&inputs).err().unwrap_or_default(),
        "The contract phase may change only contract files, files matching the stage's \
         `harness` globs, and the scaffold edits the signal lists. Revert every other \
         change (`git checkout -- <path>`, or delete the untracked file), then",
    )
}

/// Every file the contract session wrote or the freeze adds, in the forms a
/// tool names it by: relative to the working directory, relative to the
/// worktree root, and absolute; sorted and deduplicated. That is the changed
/// paths since the stage base, every contract file and every existing harness
/// file. An existing file whose only change is a scaffold declaration is
/// loom's edit, not the writer's, so it is left out.
pub(crate) fn written_paths(site: &ContractSite, repo: &WorktreeGit) -> Result<Vec<String>> {
    let stage = &site.stage;
    let base = stage_base(repo, &site.work_dir)?;
    let declared = declared_paths(site, repo, &base);
    let mut files: BTreeSet<String> = changed_paths(repo, &site.working_dir, &base)?
        .into_iter()
        .filter(|path| !declared.contains(path))
        .collect();
    files.extend(stage.contracts.iter().map(|c| normalize(&c.file)));
    files.extend(harness_files(repo, &site.working_dir, &stage.harness)?);
    let prefix = site.working_dir_prefix();
    let text = |path: PathBuf| path.to_string_lossy().into_owned();
    let forms: BTreeSet<String> = files
        .iter()
        .flat_map(|file| {
            [
                file.clone(),
                text(prefix.join(file)),
                text(site.working_dir.join(file)),
            ]
        })
        .collect();
    Ok(forms.into_iter().collect())
}

/// The existing files the scaffold edits only by declaring a module in them,
/// relative to the working directory. A scaffold that cannot be derived
/// leaves none: the change check reports that problem.
fn declared_paths(site: &ContractSite, repo: &WorktreeGit, base: &str) -> BTreeSet<String> {
    let edits = stage_scaffold(
        repo,
        &site.worktree_root,
        &site.working_dir,
        base,
        &site.stage.contracts,
    );
    edits
        .unwrap_or_default()
        .into_iter()
        .filter_map(|edit| match edit {
            ScaffoldEdit::Declare { path, .. } => Some(path),
            ScaffoldEdit::CreateRoot { .. } => None,
        })
        .collect()
}

/// The static gate: a frozen file cannot change, so a contract or harness file
/// that fails one of the stage's formatter checks, or that a lint or search
/// check names (`static_problems`), is refused before the freeze.
/// `contracts_compile` lets the compiler-backed lints run.
pub(crate) fn check_formatting(
    site: &ContractSite,
    confinement: CommandConfinement,
    written: &[String],
    contracts_compile: bool,
) -> Result<()> {
    let stage = &site.stage;
    let mut problems = format_problems(
        &stage.acceptance,
        &stage.setup,
        &site.worktree_root,
        &site.working_dir,
        confinement,
    )?;
    problems.extend(static_problems(
        stage,
        &site.worktree_root,
        &site.working_dir,
        confinement,
        written,
        contracts_compile,
    )?);
    refuse(
        &stage.id,
        problems,
        "Every contract file and harness file must pass the stage's formatter checks, and no lint or search check of the stage may name a file you wrote: run the repository's formatter over them (for Rust, `cargo fmt --all`), fix what the checks name, then",
    )
}

/// A freeze refused for the problems it lists, with the way out.
#[derive(Debug)]
struct Refusal {
    problems: Vec<String>,
    message: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

/// What a failed attempt tells the operator: the problems a refusal listed,
/// else the error the attempt failed with.
pub(crate) fn listed_problems(error: &anyhow::Error) -> Vec<String> {
    refused_problems(error).map_or_else(|| vec![format!("{error:#}")], <[String]>::to_vec)
}

/// The problems `error` lists when it is a refusal of this module; `None`
/// for any other error, such as a check that could not run.
pub(crate) fn refused_problems(error: &anyhow::Error) -> Option<&[String]> {
    error
        .downcast_ref::<Refusal>()
        .map(|refused| refused.problems.as_slice())
}

/// Fail with every problem listed, and the way out.
pub(crate) fn refuse(stage_id: &str, problems: Vec<String>, fix: &str) -> Result<()> {
    if problems.is_empty() {
        return Ok(());
    }
    let list: Vec<String> = problems
        .iter()
        .map(|problem| format!("  - {problem}"))
        .collect();
    let message = format!(
        "Contracts not frozen:\n{}\n{fix} run `loom stage contracts freeze {stage_id}` again.",
        list.join("\n")
    );
    Err(Refusal { problems, message }.into())
}

mod prepared;
pub(crate) use prepared::{prepare_contract, run_prepared, JudgedRun, PreparedContract};
