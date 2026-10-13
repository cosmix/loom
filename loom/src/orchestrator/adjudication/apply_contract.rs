//! A contract dispute's verdicts (DESIGN D15), routed here by `apply_kinds.rs`.
//!
//! `accept` means the frozen contract was wrong and the agent's edit is right:
//! the verdict's optional `contracts` amendment is applied, then the disputed
//! contract and every contract the record no longer describes are re-frozen:
//! their files at their current worktree content, their files and run
//! directories recorded (see `refrozen_contracts`). `reject` means the contract stands: the agent
//! restores it and implements against it. Neither sends the stage to a human, except an amendment that hits the
//! per-stage amendment cap, exactly as for a criterion dispute.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::fs::safe_read::read_bounded;
use crate::models::dispute::PlanPatch;
use crate::models::stage::Stage;
use crate::plan::amendment::{AmendmentField, AmendmentRequest};
use crate::verify::contracts::run_dir::resolve_run;
use crate::verify::contracts::store::{self as contract_store, FreezeRecord, RefrozenContract};
use crate::verify::contracts::{normalize, resolve_adapter, site::stage_site};

use super::apply::amend_plan;
use super::plan_patch;

/// Apply an accepted contract dispute and return the feedback for the agent,
/// or `None` when the amendment cap escalated the stage instead.
pub(super) fn accept(
    work_dir: &Path,
    stage: &mut Stage,
    dispute: u32,
    contract_id: &str,
    plan_patch: &PlanPatch,
) -> Result<Option<String>> {
    let mut files: Vec<String> = contract_file(stage, contract_id).into_iter().collect();
    if let Some(request) = contracts_amendment(&stage.id, plan_patch, dispute)? {
        if !amend_plan(work_dir, stage, request)? {
            return Ok(None);
        }
    }
    let Some(record) = contract_store::load_freeze(work_dir, &stage.id)? else {
        bail!("the contracts of stage '{}' were never frozen", stage.id);
    };
    files.extend(recorded_file(&record, contract_id));
    files.extend(contract_file(stage, contract_id));
    if files.is_empty() {
        bail!(
            "stage '{}' has no contract '{contract_id}' to re-freeze",
            stage.id
        );
    }
    let (worktree_root, working_dir) = stage_paths(work_dir, stage)?;
    let site = (worktree_root.as_path(), working_dir.as_path());
    let contracts = refrozen_contracts(stage, &record, contract_id, site)?;
    files.extend(contracts.iter().map(|contract| contract.file.clone()));
    files.sort();
    files.dedup();
    let contents = read_worktree_files(&working_dir, &files)?;
    contract_store::refreeze(work_dir, &stage.id, &contents, &contracts)?;
    Ok(Some(format!(
        "The adjudicator accepted your contract dispute: contract '{contract_id}' is \
         re-frozen at its current content ({}).\n\nAction: implement until every contract \
         passes.\n",
        files.join(", ")
    )))
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
/// unchanged. The choice reads only the record, which `refreeze` rewrites
/// last, so a retry after a partial failure makes the same one. `site` is
/// the stage's worktree root and working directory.
fn refrozen_contracts(
    stage: &Stage,
    record: &FreezeRecord,
    contract_id: &str,
    (worktree_root, working_dir): (&Path, &Path),
) -> Result<Vec<RefrozenContract>> {
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

/// The current content of `paths` (relative to `working_dir`) in the stage's
/// worktree, read without following a symlink, as the freeze read it.
fn read_worktree_files(working_dir: &Path, paths: &[String]) -> Result<Vec<(String, Vec<u8>)>> {
    paths
        .iter()
        .map(|path| {
            let bytes = read_bounded(
                working_dir,
                Path::new(path),
                contract_store::MAX_FROZEN_FILE_BYTES,
            )
            .with_context(|| format!("cannot re-freeze '{path}'"))?;
            Ok((path.clone(), bytes))
        })
        .collect()
}

/// The stage's worktree root and working directory inside it, located the
/// way the freeze handler (`daemon/server/contracts.rs`) locates them.
fn stage_paths(work_dir: &Path, stage: &Stage) -> Result<(PathBuf, PathBuf)> {
    let work_dir = work_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", work_dir.display()))?;
    stage_site(&work_dir, stage)
}
