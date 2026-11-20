//! Validating the verdict on a contract dispute: `contract-rulings`, one
//! `accept` or `reject` per disputed contract, plus an optional `plan_patch`
//! on `contracts`; or `needs-more-evidence`.
//!
//! The rules need the stage and its freeze record (which contracts a
//! replacement touches, where each contract sits in the stage's list), so the
//! verdict is checked when `loom stage adjudicate` records it
//! (`verdict::parse_and_validate_in`). Every defect is an `Err` naming it for
//! the judge to correct: nothing is recorded and no evidence round is spent.
//!
//! - every disputed contract has exactly one ruling, and no other id one;
//! - every ruling has a non-empty `reasoning` and at least one citation;
//! - every patch index is the index of a disputed contract ruled `accept`;
//! - every `accept` changes something at apply: the replacement of its frozen
//!   file is written (every contract in that file accepted), a harness file
//!   is written (every ruling accept), or a patch names its index;
//! - no `accept` deletes its contract by patch while the dispute replaces
//!   that contract's own file.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::models::dispute::{
    ContractRuling, ContractRulingKind, DisputeKind, DisputeVerdict, PlanPatch,
};
use crate::plan::amendment::{AmendmentField, AmendmentPatch};

use super::apply_contract_rulings::files_to_write;
use super::plan_patch;
use super::verdict::{
    parse_citations, validate_needs_more, verdict_tag, RecordContext, ValidationOutcome,
};

#[path = "verdict_contract_facts.rs"]
mod facts;

use facts::{dispute_facts, DisputeFacts};

/// The verdict shape a contract dispute takes, as a refusal names it.
const SHAPE: &str = "{\"verdict\": \"contract-rulings\", \"rulings\": [{\"contract\": \"<id>\", \
     \"ruling\": \"accept\"|\"reject\", \"reasoning\": \"...\", \"citations\": [...]}], \
     \"plan_patch\": {\"field\": \"contracts\", \"patches\": [{\"op\", \"index\", \"value\"}]}} \
     (plan_patch optional), or needs-more-evidence";

/// The `plan_patch` a verdict carries: its patches and reason, `None` for none.
type Patches = Option<(Vec<AmendmentPatch>, Option<String>)>;

/// The verdict `json` on the contract dispute `kind` of `context`'s stage.
pub(super) fn validate(
    json: &Value,
    kind: &DisputeKind,
    context: &RecordContext<'_>,
) -> Result<ValidationOutcome, String> {
    match verdict_tag(json).as_deref() {
        Some("needs-more-evidence" | "needs-evidence") => Ok(validate_needs_more(json)),
        Some("contract-rulings") => {
            let facts = dispute_facts(kind, context)?;
            let rulings = validate_contract_rulings(
                json,
                &facts.disputed,
                &facts.replaced,
                &facts.contract_index,
            )?;
            let patches = parse_patches(json)?;
            check_no_deleted_replaced(&rulings, patches.as_ref(), &facts)?;
            check_accepts_write(&rulings, patches.as_ref(), &facts)?;
            Ok(ValidationOutcome::Verdict(
                DisputeVerdict::ContractRulings {
                    rulings,
                    plan_patch: PlanPatch {
                        inner: canonical(patches),
                    },
                },
            ))
        }
        Some(other) => Err(format!(
            "a contract dispute does not take verdict '{other}': rule on each disputed \
             contract as {SHAPE}"
        )),
        None => Err(format!("the verdict has no 'verdict' field; write {SHAPE}")),
    }
}

/// The rulings of `json` on the contracts `disputed` names. `replaced` holds
/// the disputed contracts whose frozen file the dispute replaces;
/// `contract_index` maps each of the stage's contracts to its list index.
pub(super) fn validate_contract_rulings(
    json: &Value,
    disputed: &[String],
    replaced: &BTreeSet<String>,
    contract_index: &BTreeMap<String, usize>,
) -> Result<Vec<ContractRuling>, String> {
    let rulings = parse_rulings(json)?;
    check_coverage(&rulings, disputed)?;
    let patches = parse_patches(json)?;
    let accepted: Vec<&str> = rulings
        .iter()
        .filter(|ruling| ruling.ruling == ContractRulingKind::Accept)
        .map(|ruling| ruling.contract.as_str())
        .collect();
    let patched = patched_indices(patches.as_ref());
    check_patched_accepted(&patched, &accepted, contract_index)?;
    let idle = accepted.iter().find(|id| {
        !replaced.contains(**id)
            && contract_index
                .get(**id)
                .is_none_or(|index| !patched.contains(index))
    });
    if let Some(id) = idle {
        return Err(format!(
            "the accept on contract '{id}' changes nothing: the dispute carries no \
             replacement for its file and plan_patch has no patch at its index. Reject it, \
             or add the patch that changes it"
        ));
    }
    Ok(rulings)
}

/// The contract indices the verdict's patches name.
fn patched_indices(patches: Option<&(Vec<AmendmentPatch>, Option<String>)>) -> BTreeSet<usize> {
    patches
        .iter()
        .flat_map(|(patches, _)| patches.iter().map(AmendmentPatch::index))
        .collect()
}

/// Every `accept` changes something at apply ([`files_to_write`]): its file
/// is written, a harness file is written, or a patch names its index. An
/// accept whose file is held back because another contract in it is
/// rejected, or whose only change is a harness file one reject holds back,
/// changes nothing.
fn check_accepts_write(
    rulings: &[ContractRuling],
    patches: Option<&(Vec<AmendmentPatch>, Option<String>)>,
    facts: &DisputeFacts,
) -> Result<(), String> {
    let written = files_to_write(rulings, &facts.files, &facts.filed);
    if written
        .iter()
        .any(|file| !facts.files.values().any(|held| held == file))
    {
        return Ok(());
    }
    let patched = patched_indices(patches);
    let idle = rulings.iter().find(|ruling| {
        ruling.ruling == ContractRulingKind::Accept
            && !facts
                .contract_index
                .get(&ruling.contract)
                .is_some_and(|index| patched.contains(index))
            && !facts
                .files
                .get(&ruling.contract)
                .is_some_and(|file| written.contains(file))
    });
    match idle {
        Some(ruling) => Err(format!(
            "the accept on contract '{}' changes nothing: no replacement it allows is written \
             (a contract file only when every contract in it is accepted, a harness file only \
             when every ruling is accept) and plan_patch has no patch at its index. Reject it, \
             accept every contract of its file, or add the patch that changes it",
            ruling.contract
        )),
        None => Ok(()),
    }
}

/// No accepted contract is deleted by the verdict's patch while the dispute
/// replaces its own frozen file: the apply would write and re-freeze the file
/// of a contract the plan no longer lists.
fn check_no_deleted_replaced(
    rulings: &[ContractRuling],
    patches: Option<&(Vec<AmendmentPatch>, Option<String>)>,
    facts: &DisputeFacts,
) -> Result<(), String> {
    let deleted: BTreeSet<usize> = patches
        .iter()
        .flat_map(|(patches, _)| patches.iter())
        .filter_map(|patch| match patch {
            AmendmentPatch::Delete { index } => Some(*index),
            _ => None,
        })
        .collect();
    let clash = rulings.iter().find(|ruling| {
        ruling.ruling == ContractRulingKind::Accept
            && facts.own_replaced.contains(&ruling.contract)
            && facts
                .contract_index
                .get(&ruling.contract)
                .is_some_and(|index| deleted.contains(index))
    });
    match clash {
        Some(ruling) => Err(format!(
            "the accept on contract '{}' both deletes it (plan_patch) and writes the \
             replacement of its frozen file: reject it, or drop the delete patch or rely on \
             the replacement alone",
            ruling.contract
        )),
        None => Ok(()),
    }
}

/// Every index in `patched` names a contract in `accepted`.
fn check_patched_accepted(
    patched: &BTreeSet<usize>,
    accepted: &[&str],
    contract_index: &BTreeMap<String, usize>,
) -> Result<(), String> {
    let accepted_at: BTreeMap<usize, &str> = accepted
        .iter()
        .filter_map(|id| contract_index.get(*id).map(|index| (*index, *id)))
        .collect();
    if let Some(index) = patched
        .iter()
        .find(|index| !accepted_at.contains_key(index))
    {
        let allowed: Vec<String> = accepted_at
            .iter()
            .map(|(index, id)| format!("{index} ('{id}')"))
            .collect();
        return Err(format!(
            "plan_patch patches contracts index {index}, which is outside the accepted \
             contracts (indices: {}); patch only a disputed contract you accept",
            if allowed.is_empty() {
                "none".to_string()
            } else {
                allowed.join(", ")
            }
        ));
    }
    Ok(())
}

fn parse_rulings(json: &Value) -> Result<Vec<ContractRuling>, String> {
    let Some(items) = json.get("rulings").and_then(Value::as_array) else {
        return Err(format!(
            "a contract-rulings verdict needs a 'rulings' array: {SHAPE}"
        ));
    };
    items
        .iter()
        .enumerate()
        .map(|(i, item)| parse_ruling(i, item))
        .collect()
}

fn parse_ruling(i: usize, item: &Value) -> Result<ContractRuling, String> {
    let contract = item
        .get("contract")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ruling #{i} has no 'contract' string"))?
        .to_string();
    let ruling = match item.get("ruling").and_then(Value::as_str) {
        Some("accept") => ContractRulingKind::Accept,
        Some("reject") => ContractRulingKind::Reject,
        _ => {
            return Err(format!(
                "ruling #{i} on '{contract}' needs \"ruling\": \"accept\"|\"reject\""
            ))
        }
    };
    let reasoning = match item.get("reasoning").and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => text.to_string(),
        _ => return Err(format!("ruling #{i} on '{contract}' has no 'reasoning'")),
    };
    let citations =
        parse_citations(item.get("citations")).map_err(|e| format!("ruling #{i}: {e}"))?;
    if citations.is_empty() {
        return Err(format!(
            "ruling #{i} on '{contract}' needs at least one citation grounding it"
        ));
    }
    Ok(ContractRuling {
        contract,
        ruling,
        reasoning,
        citations,
    })
}

/// Exactly one ruling per disputed contract, and none for any other id.
fn check_coverage(rulings: &[ContractRuling], disputed: &[String]) -> Result<(), String> {
    if let Some(stray) = rulings.iter().find(|r| !disputed.contains(&r.contract)) {
        return Err(format!(
            "a ruling names '{}', which is not a disputed contract (disputed: {})",
            stray.contract,
            disputed.join(", ")
        ));
    }
    for id in disputed {
        let count = rulings.iter().filter(|r| r.contract == *id).count();
        if count != 1 {
            return Err(format!(
                "contract '{id}' has {count} rulings; give exactly one ruling per disputed \
                 contract"
            ));
        }
    }
    Ok(())
}

/// The verdict's `plan_patch` on `contracts`; absent, `null` or `{}` is none.
fn parse_patches(json: &Value) -> Result<Patches, String> {
    let raw = match json.get("plan_patch") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Object(map)) if map.is_empty() => return Ok(None),
        Some(raw) => raw,
    };
    let (field, patches, reason) = plan_patch::normalize(raw)
        .map_err(|msg| format!("plan_patch is malformed: {msg}; write {SHAPE}"))?;
    if field != AmendmentField::Contracts {
        return Err("plan_patch field must be contracts".to_string());
    }
    if let Some(index) = plan_patch::duplicate_index(&patches) {
        return Err(format!(
            "plan_patch patches contracts index {index} twice; one patch per index, each \
             naming the list as filed"
        ));
    }
    Ok(Some((patches, reason)))
}

/// The recorded `plan_patch`: canonical `patches`, or `{}` for none.
fn canonical(patches: Patches) -> Value {
    match patches {
        Some((patches, reason)) => plan_patch::canonical_patches_inner(
            AmendmentField::Contracts,
            &patches,
            reason.as_deref(),
        ),
        None => Value::Object(serde_json::Map::new()),
    }
}
