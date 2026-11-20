//! The re-freeze an accepted contract ruling ends with (DESIGN D15), driven
//! by `apply_contract_rulings.rs`.
//!
//! The stage's whole frozen set (contract and harness files) is re-frozen at
//! its current worktree content, deleted harness files dropped, and every
//! accepted contract and every contract the record no longer describes get
//! their files and run directories recorded (see `refrozen_contracts`).

use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;

use crate::models::stage::Stage;
use crate::plan::amendment::AmendmentPatch;
use crate::plan::schema::ContractSpec;
use crate::stage_service::contracts::Site;
use crate::verify::contracts::changes::harness_files;
use crate::verify::contracts::frozen_set::read_frozen_set;
use crate::verify::contracts::run_dir::resolve_run;
use crate::verify::contracts::store::{self as contract_store, FreezeRecord, RefrozenContract};
use crate::verify::contracts::{normalize, resolve_adapter};

#[path = "apply_contract_replacement.rs"]
mod replacement;
#[cfg(test)]
pub(super) use replacement::FAIL_AFTER_WRITE;
pub(super) use replacement::{check_writable, conflict, write};

/// What a re-freeze froze: the paths and their bytes, and the paths dropped
/// because the worktree no longer holds them.
pub(super) struct Refrozen {
    pub(super) files: Vec<(String, Vec<u8>)>,
    pub(super) dropped: Vec<String>,
}

/// Re-freeze the stage's whole frozen set as the worktree holds it now
/// (`frozen_set::read_frozen_set`): every contract file of the (amended)
/// stage, every harness match listed afresh, every path `record` holds, and
/// `extra`, the accepted contracts' files from before the amendment. A
/// recorded or harness path deleted from the worktree leaves the freeze. The
/// store publishes the new generation whole or not at all, so an interrupted
/// re-freeze leaves the prior snapshot committed; a retry re-reads the
/// worktree. `accepted` are the contracts ruled accept, `amended` the ids the
/// verdict's patches replaced or inserted.
pub(super) fn refreeze(
    work_dir: &Path,
    stage: &Stage,
    record: &FreezeRecord,
    (accepted, amended): (&BTreeSet<String>, &BTreeSet<String>),
    extra: &[String],
    site: &Site,
) -> Result<Refrozen> {
    let mut recorded: Vec<String> = extra.to_vec();
    recorded.extend(record.files.iter().map(|file| file.path.clone()));
    let contracts = refrozen_contracts(stage, record, accepted, amended, site)?;
    let harness = harness_files(&site.git, &site.working_dir, &stage.harness)?;
    let files = read_frozen_set(&site.working_dir, &stage.contracts, &harness, &recorded)?;
    let dropped = contract_store::refreeze_set(work_dir, &stage.id, &files, &contracts)?;
    Ok(Refrozen { files, dropped })
}

/// The contracts the re-freeze records: the accepted ones, and every one the
/// record does not describe as it now stands, because its id or file is not
/// recorded together (an amendment inserted, renamed or re-pointed it) or its
/// entry lacks a file or run directory. Each is resolved with the run
/// directory the record holds for its file (`store::recorded_cwd`), so a
/// runner config planted since cannot move a contract whose file is
/// unchanged. The choice reads only the record, which `refreeze_set` rewrites
/// last, so a retry after a partial failure makes the same one. `amended`
/// holds the ids the verdict's patch replaced or inserted (see
/// [`refrozen_spec`] for the test each one records).
fn refrozen_contracts(
    stage: &Stage,
    record: &FreezeRecord,
    accepted: &BTreeSet<String>,
    amended: &BTreeSet<String>,
    site: &Site,
) -> Result<Vec<RefrozenContract>> {
    let (worktree_root, working_dir) = (site.worktree_root.as_path(), site.working_dir.as_path());
    let mut refrozen = Vec::new();
    for contract in &stage.contracts {
        let file = normalize(&contract.file);
        if !accepted.contains(&contract.id) && is_recorded(record, &contract.id, &file) {
            continue;
        }
        let effective = refrozen_spec(record, contract, amended);
        let adapter = resolve_adapter(&effective, working_dir);
        let recorded = contract_store::recorded_cwd(record, contract);
        let run = resolve_run(&effective, adapter, worktree_root, working_dir, recorded)?;
        refrozen.push(RefrozenContract {
            id: contract.id.clone(),
            adapter: adapter.map(|adapter| adapter.name().to_string()),
            file,
            cwd: run.cwd,
            test: effective.test,
        });
    }
    Ok(refrozen)
}

/// `contract` as the re-freeze runs and records it: the record's view of it
/// ([`contract_store::effective_contract`]: the test the freeze ran, the
/// adapter it used), except that a contract the verdict's patch replaced or
/// inserted takes its amended spec's `test` when that names one. An amendment
/// that names no test leaves the writer's.
fn refrozen_spec(
    record: &FreezeRecord,
    contract: &ContractSpec,
    amended: &BTreeSet<String>,
) -> ContractSpec {
    let mut effective = contract_store::effective_contract(record, contract);
    if amended.contains(&contract.id) && contract.test.is_some() {
        effective.test = contract.test.clone();
    }
    effective
}

/// The id of the contract a replace or insert patch carries; a delete, or a
/// value that is not a contract, names none (the amendment itself rejects it).
pub(super) fn amended_ids(patch: &AmendmentPatch) -> BTreeSet<String> {
    let value = match patch {
        AmendmentPatch::Replace { value, .. } | AmendmentPatch::Insert { value, .. } => value,
        AmendmentPatch::Delete { .. } => return BTreeSet::new(),
    };
    crate::yaml::from_str::<ContractSpec>(value)
        .map(|spec| BTreeSet::from([spec.id]))
        .unwrap_or_default()
}

/// Whether `record` holds an entry for contract `id` naming `file` with a run
/// directory.
fn is_recorded(record: &FreezeRecord, id: &str, file: &str) -> bool {
    record
        .contracts
        .iter()
        .any(|entry| entry.id == id && entry.file.as_deref() == Some(file) && entry.cwd.is_some())
}

#[cfg(test)]
#[path = "apply_contract_choice_tests.rs"]
mod choice_tests;
