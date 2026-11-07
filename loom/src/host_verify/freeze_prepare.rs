//! Preparing one contract of a freeze job.

use std::path::Path;

use anyhow::{bail, Result};

use crate::plan::schema::ContractSpec;
use crate::verify::contracts::freeze::{prepare_contract, PreparedContract};

/// [`prepare_contract`] for a contract of the effective stage, refusing a test
/// the writer chose unless a test-runner adapter builds its command.
///
/// Without an adapter a test runs as a shell command, which only the plan may
/// give. The adapter is read from the live worktree, here, once per contract
/// just before it runs, so the check on this same value is the one that
/// decides how the contract runs: an earlier check on another read of the
/// tree (the writer's admission, `revalidate_saved`) cannot be undone by a
/// file the writer creates and deletes between the two reads. `planned` is
/// the stage's contracts before the writer's choices.
pub(super) fn prepare_planned(
    worktree_root: &Path,
    working_dir: &Path,
    planned: &[ContractSpec],
    contract: &ContractSpec,
) -> Result<PreparedContract> {
    let prepared = prepare_contract(worktree_root, working_dir, contract)?;
    let from_plan = planned
        .iter()
        .any(|spec| spec.id == contract.id && spec.test == contract.test);
    if !from_plan && !prepared.has_adapter() {
        bail!(
            "contract '{id}': --test needs a test-runner adapter, and none answers for {file} \
             where the contract runs: without one the test would run as a shell command, which \
             only the plan may give; name the adapter with --runner {id}=<adapter>",
            id = contract.id,
            file = contract.file
        );
    }
    Ok(prepared)
}
