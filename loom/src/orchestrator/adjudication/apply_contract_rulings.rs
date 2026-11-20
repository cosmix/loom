//! Applying a contract dispute's `contract-rulings` verdict (DESIGN D15),
//! routed here by `apply_kinds.rs`.
//!
//! `accept` means the frozen contract was wrong and the change the dispute
//! proposes for it is right; `reject` means the contract stands. A
//! replacement of a contract file is written only when every contract in
//! that file is accepted, a harness replacement only when every ruling is an
//! accept ([`files_to_write`]). With no accept nothing changes: the agent is
//! told why and the stage re-queues. Otherwise, in order:
//!
//! 1. every file to write is checked against the freeze record
//!    (`apply_contract_replacement::conflict`); any conflict writes nothing
//!    and sends the stage to human review;
//! 2. every file to write is checked for what the write refuses (a symlink,
//!    a target that is not a regular file), so a refusal leaves the plan
//!    unamended;
//! 3. the verdict's `contracts` patches are applied, every one or none past
//!    the per-stage amendment cap (which escalates, as for a criterion);
//! 4. each file is written;
//! 5. the stage's whole frozen set is re-frozen once (`apply_contract::refreeze`).
//!
//! A replay after a crash finds some files already equal to their
//! replacement: step 1 admits them and step 4 rewrites the same bytes.

use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::models::dispute::{
    ContractRuling, ContractRulingKind, DisputeKind, FileReplacement, PlanPatch,
};
use crate::models::stage::Stage;
use crate::plan::amendment::AmendmentRequest;
use crate::stage_service::contracts::Site;
use crate::verify::contracts::store::{contract_file, contract_files, load_freeze, FreezeRecord};

use super::apply::{amend_plan, escalate_amendment_cap};
use super::apply_contract::{amended_ids, check_writable, conflict, refreeze, write, Refrozen};
use super::apply_kinds::notify_and_requeue;
use super::plan_patch::build_amendment_requests;

#[path = "apply_contract_notice.rs"]
mod notice;

/// Apply `rulings` (and `plan_patch`) on the contract dispute `kind`.
pub(super) fn apply(
    work_dir: &Path,
    stage: &mut Stage,
    dispute: u32,
    kind: &DisputeKind,
    (rulings, plan_patch): (&[ContractRuling], &PlanPatch),
) -> Result<()> {
    let DisputeKind::Contract {
        contract_ids,
        replacements,
    } = kind
    else {
        bail!(
            "dispute {dispute} of stage '{}' is not a contract dispute",
            stage.id
        );
    };
    if !rulings.iter().any(is_accept) {
        let body = notice::rejected(&stage.id, rulings, !replacements.is_empty());
        return notify_and_requeue(work_dir, stage, &body);
    }
    let Some(record) = load_freeze(work_dir, &stage.id)? else {
        bail!("the contracts of stage '{}' were never frozen", stage.id);
    };
    let (requests, amended) = amendment_requests(&stage.id, plan_patch, dispute)?;
    let contract_files = disputed_contract_files(stage, &record, contract_ids, &amended);
    let filed: Vec<String> = replacements.iter().map(|r| r.file.clone()).collect();
    let to_write = files_to_write(rulings, &contract_files, &filed);
    let writes: Vec<&FileReplacement> = replacements
        .iter()
        .filter(|replacement| to_write.contains(&replacement.file))
        .collect();
    let Some(site) = checked_site(work_dir, stage, &record, &writes, dispute)? else {
        return Ok(());
    };
    if !amend(work_dir, stage, requests)? {
        return Ok(());
    }
    let written = (&writes[..], &amended);
    let refrozen = write_and_refreeze(work_dir, stage, (&record, &site), rulings, written)?;
    let outcome = notice::Outcome {
        rulings,
        refrozen: &refrozen,
        written: &to_write,
        filed: &filed,
        contract_files: &contract_files,
    };
    notify_and_requeue(work_dir, stage, &notice::accepted(&outcome))
}

/// The site to write `writes` at, checked before anything changes: `None`
/// when one conflicts with the freeze record (the stage escalates), an error
/// when the write would refuse one.
fn checked_site(
    work_dir: &Path,
    stage: &mut Stage,
    record: &FreezeRecord,
    writes: &[&FileReplacement],
    dispute: u32,
) -> Result<Option<Site>> {
    if let Some(reason) = writes.iter().find_map(|r| conflict(record, r, dispute)) {
        escalate_amendment_cap(work_dir, stage, &anyhow::anyhow!(reason));
        return Ok(None);
    }
    let site = super::locate_stage_site(work_dir, stage)?;
    for replacement in writes {
        check_writable(&site, &replacement.file)?;
    }
    Ok(Some(site))
}

/// Write `writes` into the worktree at `site`, then re-freeze the stage's
/// frozen set once, recording the accepted contracts and the ids `amended`
/// names.
fn write_and_refreeze(
    work_dir: &Path,
    stage: &Stage,
    (record, site): (&FreezeRecord, &Site),
    rulings: &[ContractRuling],
    (writes, amended): (&[&FileReplacement], &BTreeSet<String>),
) -> Result<Refrozen> {
    for replacement in writes {
        write(site, &replacement.file, &replacement.content)?;
    }
    let accepted: BTreeSet<String> = rulings
        .iter()
        .filter(|ruling| is_accept(ruling))
        .map(|ruling| ruling.contract.clone())
        .collect();
    let extra: Vec<String> = accepted
        .iter()
        .filter_map(|id| contract_file(Some(record), &stage.contracts, id))
        .collect();
    refreeze(work_dir, stage, record, (&accepted, amended), &extra, site)
}

/// The replacement files, in `replacements` order, that `rulings` let the
/// daemon write. A file holding at least one contract (`contract_files` maps
/// each contract id to its file) is written when every contract in it is
/// ruled accept; a file holding none (a harness file) when every ruling is an
/// accept.
pub(super) fn files_to_write(
    rulings: &[ContractRuling],
    contract_files: &BTreeMap<String, String>,
    replacements: &[String],
) -> Vec<String> {
    let accepted = |id: &str| {
        rulings
            .iter()
            .any(|ruling| ruling.contract == id && is_accept(ruling))
    };
    let all_accepted = rulings.iter().all(is_accept);
    replacements
        .iter()
        .filter(|file| {
            let mut holders = contract_files
                .iter()
                .filter(|(_, held)| held == file)
                .map(|(id, _)| id.as_str())
                .peekable();
            if holders.peek().is_none() {
                all_accepted
            } else {
                holders.all(accepted)
            }
        })
        .cloned()
        .collect()
}

fn is_accept(ruling: &ContractRuling) -> bool {
    ruling.ruling == ContractRulingKind::Accept
}

/// The stage's current contracts with their files ([`contract_files`], the
/// map admission and recording use), plus each disputed id with its recorded
/// file: on a replay that finds the verdict's amendment already applied, a
/// disputed contract it deleted or renamed still holds its file. An id the
/// verdict's own patch introduces (`amended`) holds no file unless it is
/// disputed: on that replay it has no ruling, and would hold back the
/// replacement the judge accepted.
fn disputed_contract_files(
    stage: &Stage,
    record: &FreezeRecord,
    disputed: &[String],
    amended: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut files = contract_files(Some(record), &stage.contracts);
    files.retain(|id, _| !amended.contains(id) || disputed.contains(id));
    for id in disputed {
        if let Some(file) = contract_file(Some(record), &stage.contracts, id) {
            files.entry(id.clone()).or_insert(file);
        }
    }
    files
}

/// The amendments the verdict's `contracts` patches make, and the ids they
/// replace or insert; none for an empty `plan_patch` object.
fn amendment_requests(
    stage_id: &str,
    plan_patch: &PlanPatch,
    dispute: u32,
) -> Result<(Vec<AmendmentRequest>, BTreeSet<String>)> {
    let inner = &plan_patch.inner;
    if inner.is_null() || inner.as_object().is_some_and(|object| object.is_empty()) {
        return Ok((Vec::new(), BTreeSet::new()));
    }
    let requests = build_amendment_requests(stage_id, plan_patch, &dispute.to_string())?;
    let amended = requests
        .iter()
        .flat_map(|request| amended_ids(&request.patch))
        .collect();
    Ok((requests, amended))
}

/// Apply `requests`; `false` when the amendment cap escalated the stage
/// instead.
fn amend(work_dir: &Path, stage: &mut Stage, requests: Vec<AmendmentRequest>) -> Result<bool> {
    for request in requests {
        if !amend_plan(work_dir, stage, request)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "apply_contract_rulings_tests.rs"]
mod tests;
