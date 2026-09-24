//! A contract dispute's verdicts (DESIGN D15), routed here by `apply_kinds.rs`.
//!
//! `accept` means the frozen contract was wrong and the agent's edit is right:
//! the verdict's optional `contracts` amendment is applied, then the
//! contract's file is re-frozen at its current worktree content. `reject`
//! means the contract stands: the agent restores it and implements against
//! it. Neither sends the stage to a human, except an amendment that hits the
//! per-stage amendment cap, exactly as for a criterion dispute.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::fs::safe_read::read_bounded;
use crate::models::dispute::PlanPatch;
use crate::models::stage::Stage;
use crate::plan::amendment::{AmendmentField, AmendmentRequest};
use crate::verify::contracts::{normalize, site::stage_site, store as contract_store};

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
        files.extend(contract_file(stage, contract_id));
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        bail!(
            "stage '{}' has no contract '{contract_id}' to re-freeze",
            stage.id
        );
    }
    let contents = read_worktree_files(work_dir, stage, &files)?;
    contract_store::refreeze(work_dir, &stage.id, &contents)?;
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

/// The current content of `paths` (relative to the working directory) in the
/// stage's worktree, read without following a symlink, as the freeze read it.
fn read_worktree_files(
    work_dir: &Path,
    stage: &Stage,
    paths: &[String],
) -> Result<Vec<(String, Vec<u8>)>> {
    let working_dir = stage_working_dir(work_dir, stage)?;
    paths
        .iter()
        .map(|path| {
            let bytes = read_bounded(
                &working_dir,
                Path::new(path),
                contract_store::MAX_FROZEN_FILE_BYTES,
            )
            .with_context(|| format!("cannot re-freeze '{path}'"))?;
            Ok((path.clone(), bytes))
        })
        .collect()
}

/// The stage's working directory inside its worktree, located the way the
/// freeze handler (`daemon/server/contracts.rs`) locates it.
fn stage_working_dir(work_dir: &Path, stage: &Stage) -> Result<PathBuf> {
    let work_dir = work_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", work_dir.display()))?;
    let (_worktree_root, working_dir) = stage_site(&work_dir, stage)?;
    Ok(working_dir)
}
