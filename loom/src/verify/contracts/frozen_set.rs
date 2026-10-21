//! The files a freeze holds: every contract file, every existing harness
//! match and, on a re-freeze, every path the record already holds. The first
//! freeze (`stage_service/contracts.rs`) and an accepted contract dispute
//! (`orchestrator/adjudication/apply_contract.rs`) both read the set here, so
//! the two agree on what is frozen and within which bounds.

use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::path::Path;

use super::normalize;
use super::store::MAX_FROZEN_FILE_BYTES;
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::plan::schema::ContractSpec;

/// Bounds on what one freeze copies into `.loom/work`: the harness globs
/// come from the plan, but the files they match are the agent's.
const MAX_FROZEN_FILES: usize = 500;
const MAX_FROZEN_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// The content of every file of `contracts` (each must exist), of every path
/// in `harness` (the glob matches, listed by `changes::harness_files`) and of
/// every path in `recorded`, sorted by path. A harness or recorded path that
/// no longer exists is left out. All paths are relative to `working_dir` and
/// read without following a symlink anywhere beneath it.
pub fn read_frozen_set(
    working_dir: &Path,
    contracts: &[ContractSpec],
    harness: &[String],
    recorded: &[String],
) -> Result<Vec<(String, Vec<u8>)>> {
    let contract_files: BTreeSet<String> = contracts.iter().map(|c| normalize(&c.file)).collect();
    let paths: BTreeSet<&String> = contract_files
        .iter()
        .chain(harness)
        .chain(recorded)
        .collect();
    if paths.len() > MAX_FROZEN_FILES {
        bail!(
            "{} files match the contracts and harness; at most {MAX_FROZEN_FILES} can be frozen",
            paths.len()
        );
    }
    let mut files = Vec::with_capacity(paths.len());
    let mut total = 0usize;
    for path in paths {
        let bytes = match read_bounded(working_dir, Path::new(path), MAX_FROZEN_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if is_not_found(&error) && !contract_files.contains(path) => continue,
            Err(error) => return Err(error.context(format!("cannot freeze '{path}'"))),
        };
        total += bytes.len();
        if total > MAX_FROZEN_TOTAL_BYTES {
            bail!("the frozen files exceed {MAX_FROZEN_TOTAL_BYTES} bytes together");
        }
        files.push((path.clone(), bytes));
    }
    Ok(files)
}
