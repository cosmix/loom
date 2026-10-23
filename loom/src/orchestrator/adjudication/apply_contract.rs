//! A contract dispute's verdicts (DESIGN D15), routed here by `apply_kinds.rs`.
//!
//! `accept` means the frozen contract was wrong and the agent's edit is right:
//! the verdict's optional `contracts` amendment is applied, then the stage's
//! whole frozen set (contract and harness files) is re-frozen at its current
//! worktree content, deleted harness files dropped, and the disputed contract
//! and every contract the record no longer describes get their files and run
//! directories recorded (see `refrozen_contracts`). `reject` means the
//! contract stands: the agent restores it and implements against it. Neither
//! sends the stage to a human, except an amendment that hits the per-stage
//! amendment cap, exactly as for a criterion dispute.

use anyhow::{bail, Result};
use std::path::Path;

use crate::models::dispute::PlanPatch;
use crate::models::stage::Stage;
use crate::plan::amendment::{AmendmentField, AmendmentRequest};
use crate::stage_service::contracts::Site;
use crate::verify::contracts::changes::harness_files;
use crate::verify::contracts::frozen_set::read_frozen_set;
use crate::verify::contracts::run_dir::resolve_run;
use crate::verify::contracts::store::{self as contract_store, FreezeRecord, RefrozenContract};
use crate::verify::contracts::{normalize, resolve_adapter};

use super::apply::amend_plan;
use super::plan_patch;

/// Apply an accepted contract dispute and return the feedback for the agent,
/// or `None` when the amendment cap escalated the stage instead.
///
/// The re-freeze covers the stage's whole frozen set as the worktree holds it
/// now (`frozen_set::read_frozen_set`): every contract file of the amended
/// stage, every harness match listed afresh, and every path the record holds
/// plus the disputed contract's file from before the amendment. A recorded or
/// harness path deleted from the worktree leaves the freeze. The set derives
/// from the stage, the record's paths and the worktree, so a retry after a
/// partial failure writes the same record.
pub(super) fn accept(
    work_dir: &Path,
    stage: &mut Stage,
    dispute: u32,
    contract_id: &str,
    plan_patch: &PlanPatch,
) -> Result<Option<String>> {
    let mut recorded: Vec<String> = contract_file(stage, contract_id).into_iter().collect();
    if let Some(request) = contracts_amendment(&stage.id, plan_patch, dispute)? {
        if !amend_plan(work_dir, stage, request)? {
            return Ok(None);
        }
    }
    let Some(record) = contract_store::load_freeze(work_dir, &stage.id)? else {
        bail!("the contracts of stage '{}' were never frozen", stage.id);
    };
    recorded.extend(recorded_file(&record, contract_id));
    recorded.extend(contract_file(stage, contract_id));
    if recorded.is_empty() {
        bail!(
            "stage '{}' has no contract '{contract_id}' to re-freeze",
            stage.id
        );
    }
    recorded.extend(record.files.iter().map(|file| file.path.clone()));
    let site = super::locate_stage_site(work_dir, stage)?;
    let contracts = refrozen_contracts(stage, &record, contract_id, &site)?;
    let harness = harness_files(&site.git, &site.working_dir, &stage.harness)?;
    let files = read_frozen_set(&site.working_dir, &stage.contracts, &harness, &recorded)?;
    let dropped = contract_store::refreeze_set(work_dir, &stage.id, &files, &contracts)?;
    Ok(Some(accept_notice(contract_id, &files, &dropped)))
}

/// The feedback for an accepted contract dispute: the paths re-frozen and
/// the ones dropped from the freeze.
fn accept_notice(contract_id: &str, files: &[(String, Vec<u8>)], dropped: &[String]) -> String {
    let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    let dropped = if dropped.is_empty() {
        String::new()
    } else {
        format!(
            " Dropped from the freeze, as the worktree no longer holds them: {}.",
            dropped.join(", ")
        )
    };
    format!(
        "The adjudicator accepted your contract dispute on contract '{contract_id}': the \
         stage's contract and harness files are re-frozen at their current content ({}).\
         {dropped}\n\nAction: implement until every contract passes.\n",
        paths.join(", ")
    )
}

/// The feedback for a rejected contract dispute.
pub(super) fn reject_notice(stage_id: &str, contract_id: &str, reasoning: &str) -> String {
    format!(
        "The adjudicator rejected your contract dispute: contract '{contract_id}' stands as \
         frozen.\n\n### Reasoning\n\n{}\n\nAction: restore the frozen contract with \
         `loom stage contracts restore {stage_id} --contract {contract_id}`, then implement \
         against it.\n",
        reasoning.trim()
    )
}

/// The frozen file of `contract_id`, as the freeze names it.
fn contract_file(stage: &Stage, contract_id: &str) -> Option<String> {
    stage
        .contracts
        .iter()
        .find(|contract| contract.id == contract_id)
        .map(|contract| normalize(&contract.file))
}

/// The `contracts` amendment an accept carries; an empty `plan_patch` object
/// (see `verdict_kinds.rs`) carries none.
fn contracts_amendment(
    stage_id: &str,
    plan_patch: &PlanPatch,
    dispute: u32,
) -> Result<Option<AmendmentRequest>> {
    let inner = &plan_patch.inner;
    if inner.is_null() || inner.as_object().is_some_and(|object| object.is_empty()) {
        return Ok(None);
    }
    let (patch, reason) = plan_patch::decode_patch(inner).map_err(|msg| anyhow::anyhow!(msg))?;
    Ok(Some(AmendmentRequest {
        stage_id: stage_id.to_string(),
        field: AmendmentField::Contracts,
        patch,
        reason,
        dispute_id: Some(dispute.to_string()),
    }))
}

/// The contracts the re-freeze records: the disputed one, and every one the
/// record does not describe as it now stands, because its id or file is not
/// recorded together (an amendment inserted, renamed or re-pointed it) or its
/// entry lacks a file or run directory. Each is resolved with the run
/// directory the record holds for its file (`store::recorded_cwd`), so a
/// runner config planted since cannot move a contract whose file is
/// unchanged. The choice reads only the record, which `refreeze_set` rewrites
/// last, so a retry after a partial failure makes the same one.
fn refrozen_contracts(
    stage: &Stage,
    record: &FreezeRecord,
    contract_id: &str,
    site: &Site,
) -> Result<Vec<RefrozenContract>> {
    let (worktree_root, working_dir) = (site.worktree_root.as_path(), site.working_dir.as_path());
    let mut refrozen = Vec::new();
    for contract in &stage.contracts {
        let file = normalize(&contract.file);
        if contract.id != contract_id && is_recorded(record, &contract.id, &file) {
            continue;
        }
        let adapter = resolve_adapter(contract, working_dir);
        let recorded = contract_store::recorded_cwd(record, contract);
        let run = resolve_run(contract, adapter, worktree_root, working_dir, recorded)?;
        refrozen.push(RefrozenContract {
            id: contract.id.clone(),
            adapter: adapter.map(|adapter| adapter.name().to_string()),
            file,
            cwd: run.cwd,
        });
    }
    Ok(refrozen)
}

/// Whether `record` holds an entry for contract `id` naming `file` with a run
/// directory.
fn is_recorded(record: &FreezeRecord, id: &str, file: &str) -> bool {
    record
        .contracts
        .iter()
        .any(|entry| entry.id == id && entry.file.as_deref() == Some(file) && entry.cwd.is_some())
}

/// The file `record` holds for contract `id`. For the disputed contract it is
/// the file from before the amendment, even when a retry finds the amendment
/// already applied.
fn recorded_file(record: &FreezeRecord, id: &str) -> Option<String> {
    record
        .contracts
        .iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.file.clone())
}
