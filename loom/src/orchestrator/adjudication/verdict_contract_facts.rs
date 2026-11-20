//! What a contract dispute contests, gathered for `verdict_contract`: from
//! the dispute's kind, the stage and its freeze record.

use std::collections::{BTreeMap, BTreeSet};

use crate::models::dispute::{DisputeKind, FileReplacement};
use crate::verify::contracts::store::{contract_files, load_freeze};

use super::super::verdict::RecordContext;

/// What the dispute contests, from its kind, the stage and the freeze record.
pub(super) struct DisputeFacts {
    /// The disputed contract ids.
    pub(super) disputed: Vec<String>,
    /// The disputed contracts whose own frozen file a replacement names.
    pub(super) own_replaced: BTreeSet<String>,
    /// The disputed contracts an accept may write for: `own_replaced`, or
    /// every disputed contract when a replacement names a file holding no
    /// contract (a harness file, written only when every ruling is accept).
    pub(super) replaced: BTreeSet<String>,
    /// Each of the stage's contracts' list index.
    pub(super) contract_index: BTreeMap<String, usize>,
    /// Each of the stage's contracts' frozen file.
    pub(super) files: BTreeMap<String, String>,
    /// The files the dispute's replacements name, in filing order.
    pub(super) filed: Vec<String>,
}

pub(super) fn dispute_facts(
    kind: &DisputeKind,
    context: &RecordContext<'_>,
) -> Result<DisputeFacts, String> {
    let DisputeKind::Contract {
        contract_ids,
        replacements,
    } = kind
    else {
        return Err("the dispute is not a contract dispute".to_string());
    };
    let stage = context.stage;
    let record = load_freeze(context.work_dir, &stage.id).map_err(|e| {
        format!(
            "the freeze record of stage '{}' is unreadable: {e:#}",
            stage.id
        )
    })?;
    let files = contract_files(record.as_ref(), &stage.contracts);
    let (own_replaced, replaced) = replaced_sets(&files, contract_ids, replacements);
    let contract_index = stage
        .contracts
        .iter()
        .enumerate()
        .map(|(index, spec)| (spec.id.clone(), index))
        .collect();
    Ok(DisputeFacts {
        disputed: contract_ids.clone(),
        own_replaced,
        replaced,
        contract_index,
        files,
        filed: replacements.iter().map(|r| r.file.clone()).collect(),
    })
}

/// The disputed ids whose own file `replacements` name, and the ids an accept
/// may write for (all of them once a replaced file holds no contract of
/// `files`, the stage's contract-to-file map).
fn replaced_sets(
    files: &BTreeMap<String, String>,
    contract_ids: &[String],
    replacements: &[FileReplacement],
) -> (BTreeSet<String>, BTreeSet<String>) {
    let replaced_files: BTreeSet<&str> = replacements.iter().map(|r| r.file.as_str()).collect();
    let own: BTreeSet<String> = contract_ids
        .iter()
        .filter(|id| {
            files
                .get(id.as_str())
                .is_some_and(|file| replaced_files.contains(file.as_str()))
        })
        .cloned()
        .collect();
    let harness_replaced = replaced_files
        .iter()
        .any(|file| !files.values().any(|held| held == file));
    if harness_replaced {
        (own, contract_ids.iter().cloned().collect())
    } else {
        (own.clone(), own)
    }
}
