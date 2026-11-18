//! The writer's `--test`, `--runner` and `--cwd` choices at the daemon: they
//! are admitted with the freeze request, saved before its job is queued, and
//! recorded in the freeze.
//!
//! Admission validates the request's choices merged with the pruned saved
//! ones against the stage's base commit, through the daemon's pinned git, so
//! a `.git` file the writer rewrote cannot choose git's configuration. The
//! choices are saved inside the stage update that queues the job, so a queued
//! job always loads the choices it was requested with.

use anyhow::Result;
use std::path::Path;

use super::Site;
use crate::daemon::ContractRunReport;
use crate::host_verify::{store as jobs, JobKind};
use crate::models::stage::Stage;
use crate::plan::schema::{ContractChoice, ContractChoices, ContractSpec};
use crate::verify::contracts::choices::{
    load_choices, merge_choices, prune_choices, save_choices, validate_choices_in,
};
use crate::verify::contracts::run_dir::resolve_run;
use crate::verify::contracts::store::FrozenContract;
use crate::verify::contracts::{changes, normalize, resolve_adapter};

/// Why a request that would join a running freeze under other choices is
/// refused: a changed `choices.json` would reach the running job and break its
/// binding. The relay drain settles this exact text without filing a dispute:
/// waiting and freezing again is the remedy, not a standing refusal.
pub(crate) const FREEZE_BUSY: &str =
    "a freeze run for this tree is in progress under other flags; wait for its \
                    Host job result, then run the freeze again with the new flags";

/// The request's choices merged with the pruned saved ones, validated against
/// the stage's base commit with the daemon's pinned git; `Err` is the refusal
/// text. The request's values are stored trimmed, as the rules judge them: the
/// wire value is not trusted to have been trimmed by the CLI.
pub(super) fn admit_choices(
    work_dir: &Path,
    stage: &Stage,
    site: &Site,
    update: &ContractChoices,
) -> Result<ContractChoices> {
    let saved = prune_choices(&stage.contracts, &load_choices(work_dir, &stage.id)?);
    let merged = merge_choices(&saved, &trimmed(update));
    validate_at_base(work_dir, &stage.contracts, site, &merged)?;
    Ok(merged)
}

/// The saved choices of `stage_id`, pruned against the plan's `contracts`,
/// validated again for the tree `site` holds. The freeze job calls it once the
/// tree is bound to the request's digest: admission judged the worktree as it
/// stood then, and a file that made a choice pass (a runner's marker) may be
/// gone from the tree the job runs on. `Err` is the refusal text.
pub(crate) fn revalidate_saved(
    work_dir: &Path,
    stage_id: &str,
    contracts: &[ContractSpec],
    site: &Site,
) -> Result<()> {
    let saved = prune_choices(contracts, &load_choices(work_dir, stage_id)?);
    validate_at_base(work_dir, contracts, site, &saved)
}

fn validate_at_base(
    work_dir: &Path,
    contracts: &[ContractSpec],
    site: &Site,
    choices: &ContractChoices,
) -> Result<()> {
    let base = changes::stage_base(&site.git, work_dir)?;
    let (root, working) = (&site.worktree_root, &site.working_dir);
    validate_choices_in(&site.git, contracts, choices, root, working, &base)
}

/// `choices` with every value trimmed.
fn trimmed(choices: &ContractChoices) -> ContractChoices {
    let trim = |value: &Option<String>| value.as_deref().map(|v| v.trim().to_string());
    choices
        .iter()
        .map(|(id, choice)| {
            let choice = ContractChoice {
                test: trim(&choice.test),
                runner: trim(&choice.runner),
                cwd: trim(&choice.cwd),
            };
            (id.clone(), choice)
        })
        .collect()
}

/// Inside the `update_stage` closure: `Some(message)` when a pending `Freeze`
/// job for the same head and tree runs under other saved choices; otherwise
/// saves `merged` when it differs from the file, and answers `None`. A
/// request for another head or tree supersedes the running job, so its
/// changed choices are saved.
pub(super) fn save_admitted(
    work_dir: &Path,
    stage: &Stage,
    head: &str,
    tree_digest: &str,
    merged: &ContractChoices,
) -> Result<Option<String>> {
    let on_file = load_choices(work_dir, &stage.id)?;
    let changed = *merged != prune_choices(&stage.contracts, &on_file);
    if changed && pending_freeze(work_dir, &stage.id, head, tree_digest)? {
        return Ok(Some(FREEZE_BUSY.to_string()));
    }
    if *merged != on_file {
        save_choices(work_dir, &stage.id, merged)?;
    }
    Ok(None)
}

/// Whether the stage's current job is a non-terminal freeze of `head` and
/// `tree_digest`, which a freeze request for them joins.
fn pending_freeze(work_dir: &Path, stage_id: &str, head: &str, tree_digest: &str) -> Result<bool> {
    Ok(jobs::load_current(work_dir, stage_id)?.is_some_and(|job| {
        job.kind == JobKind::Freeze
            && !job.state.is_terminal()
            && job.head == head
            && job.tree_digest == tree_digest
    }))
}

/// Each contract's run report with its file, the directory it runs from and
/// the test it ran, which the completion check then runs the same way while
/// the contract names that file. `stage` carries the effective contracts
/// (`choices::effective_contracts`), so the writer's runner and cwd are
/// honoured and every contract has a test. The directory is resolved here as
/// the host run resolved it.
pub(super) fn frozen_contracts(
    site: &Site,
    stage: &Stage,
    reports: &[ContractRunReport],
) -> Result<Vec<FrozenContract>> {
    let mut frozen = Vec::with_capacity(stage.contracts.len());
    for contract in &stage.contracts {
        let Some(report) = reports.iter().find(|r| r.contract_id == contract.id) else {
            continue;
        };
        let adapter = resolve_adapter(contract, &site.working_dir);
        let run = resolve_run(
            contract,
            adapter,
            &site.worktree_root,
            &site.working_dir,
            None,
        )?;
        frozen.push(FrozenContract {
            file: Some(normalize(&contract.file)),
            cwd: Some(run.cwd),
            test: Some(contract.required_test()?.to_string()),
            ..FrozenContract::from(report)
        });
    }
    Ok(frozen)
}

#[cfg(test)]
#[path = "contracts_choices_tests.rs"]
mod tests;
