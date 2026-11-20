//! What a freeze record says about one plan contract: the run directory the
//! freeze ran it from and the spec completion and the readers run it with.

use std::collections::BTreeMap;

use super::normalize;
use super::store::{FreezeRecord, FrozenContract};
use crate::plan::schema::ContractSpec;

/// The entry of `record` that still describes `contract`: the entry with its id
/// and file, else any entry with its file (an amendment renamed the contract),
/// else an entry with its id that predates recording files.
fn entry_for<'a>(record: &'a FreezeRecord, contract: &ContractSpec) -> Option<&'a FrozenContract> {
    let file = normalize(&contract.file);
    let same_file = |entry: &&FrozenContract| entry.file.as_deref() == Some(file.as_str());
    let entries = &record.contracts;
    entries
        .iter()
        .filter(same_file)
        .find(|entry| entry.id == contract.id)
        .or_else(|| entries.iter().find(same_file))
        .or_else(|| {
            entries
                .iter()
                .find(|entry| entry.id == contract.id && entry.file.is_none())
        })
}

/// The run directory `record` holds for `contract`, read from the entry that
/// still describes the contract's file: the entry with its id and file, else
/// any entry with its file (an amendment renamed the contract), else an entry
/// with its id that predates recording files. `None` when no entry describes
/// the contract or the one that does recorded no run directory; the caller
/// then resolves the directory from the plan.
pub fn recorded_cwd<'a>(record: &'a FreezeRecord, contract: &ContractSpec) -> Option<&'a str> {
    entry_for(record, contract).and_then(|entry| entry.cwd.as_deref())
}

/// `spec` as the freeze ran it. `test` is the entry's recorded test, else the
/// spec's; `runner` is the spec's, else the entry's adapter. `cwd` stays as the
/// spec has it: completion already runs from [`recorded_cwd`]. A spec no entry
/// describes is returned unchanged.
pub fn effective_contract(record: &FreezeRecord, spec: &ContractSpec) -> ContractSpec {
    let Some(entry) = entry_for(record, spec) else {
        return spec.clone();
    };
    ContractSpec {
        test: entry.test.clone().or_else(|| spec.test.clone()),
        runner: spec.runner.clone().or_else(|| entry.adapter.clone()),
        ..spec.clone()
    }
}

/// The frozen file of contract `id`: the file `record` holds for it (from
/// before any amendment, even when a retry finds the amendment applied), else
/// the file `contracts` (the stage's list) names.
pub fn contract_file(
    record: Option<&FreezeRecord>,
    contracts: &[ContractSpec],
    id: &str,
) -> Option<String> {
    record
        .and_then(|record| record.contracts.iter().find(|entry| entry.id == id))
        .and_then(|entry| entry.file.clone())
        .or_else(|| {
            contracts
                .iter()
                .find(|contract| contract.id == id)
                .map(|contract| normalize(&contract.file))
        })
}

/// Each of the stage's current `contracts` with its [`contract_file`]. A
/// contract an accepted amendment deleted stays in the record but is not
/// listed: dispute admission, verdict recording and the apply of a contract
/// dispute share this map, so they agree on which contracts a file holds and
/// which replaced file is a harness file.
pub fn contract_files(
    record: Option<&FreezeRecord>,
    contracts: &[ContractSpec],
) -> BTreeMap<String, String> {
    contracts
        .iter()
        .filter_map(|spec| {
            contract_file(record, contracts, &spec.id).map(|file| (spec.id.clone(), file))
        })
        .collect()
}

#[cfg(test)]
#[path = "record_view_tests.rs"]
mod tests;
