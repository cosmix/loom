//! The replacement file an accepted contract dispute writes into the worktree.
//!
//! The bytes come from the admitted dispute record, never from the worktree, so
//! every apply attempt writes what the judge reviewed. The frozen file's hash
//! must still be the one the dispute was filed against (or already the
//! replacement's, when an earlier attempt of the same accept got further), so a
//! replay is idempotent and an accept never overwrites a file that changed
//! after filing.

use anyhow::{bail, Context, Result};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::fs::safe_fs::{safe_create_dir_all_in_workdir, safe_open_dirfd};
use crate::fs::safe_replace::restore_no_follow;
use crate::models::dispute::DisputeKind;
use crate::models::stage::Stage;
use crate::stage_service::contracts::Site;
use crate::utils::sha256_hex;
use crate::verify::contracts::store::{load_freeze, validate_relative};

use super::super::apply::escalate_amendment_cap;
use super::{contract_file, recorded_file};

/// Mode of a replacement whose target file does not exist yet.
const DEFAULT_MODE: u32 = 0o644;

/// The replacement an admitted dispute carries.
pub(crate) struct Proposed<'a> {
    pub(crate) text: &'a str,
    /// The frozen file's sha256 the dispute was filed against.
    pub(crate) base_sha256: Option<&'a str>,
}

impl<'a> Proposed<'a> {
    /// The replacement `kind` carries; `None` for a dispute without one.
    pub(crate) fn of(kind: &'a DisputeKind) -> Option<Self> {
        let DisputeKind::Contract {
            replacement: Some(text),
            replacement_base_sha256,
            ..
        } = kind
        else {
            return None;
        };
        Some(Self {
            text,
            base_sha256: replacement_base_sha256.as_deref(),
        })
    }
}

#[cfg(test)]
thread_local! {
    /// Test seam: fail once right after the write, as a crash before the
    /// re-freeze would.
    pub(super) static FAIL_AFTER_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Why the replacement must not be written, or `None` when the frozen file is
/// still what the dispute was filed against or already is the replacement.
/// Runs before the amendment and before any write; the file is the one the
/// freeze record names for the contract (the amendment may re-point it).
fn conflict(
    work_dir: &Path,
    stage: &Stage,
    contract_id: &str,
    dispute: u32,
    proposed: &Proposed<'_>,
) -> Result<Option<String>> {
    let Some(record) = load_freeze(work_dir, &stage.id)? else {
        bail!("the contracts of stage '{}' were never frozen", stage.id);
    };
    let Some(file) =
        recorded_file(&record, contract_id).or_else(|| contract_file(stage, contract_id))
    else {
        bail!(
            "stage '{}' has no contract '{contract_id}' to replace",
            stage.id
        );
    };
    let frozen = record
        .files
        .iter()
        .find(|frozen| frozen.path == file)
        .map(|frozen| frozen.sha256.as_str());
    let replacement = sha256_hex(proposed.text.as_bytes());
    let unchanged =
        frozen.is_some_and(|hash| hash == replacement || Some(hash) == proposed.base_sha256);
    Ok((!unchanged).then(|| {
        format!(
            "the frozen `{file}` changed after dispute {dispute} was filed; its replacement was \
             not written"
        )
    }))
}

/// Whether the replacement conflicts with the frozen file, in which case
/// nothing is applied and the stage is escalated to human review, as for an
/// amendment over its cap.
pub(super) fn escalated_on_conflict(
    work_dir: &Path,
    stage: &mut Stage,
    contract_id: &str,
    dispute: u32,
    proposed: &Proposed<'_>,
) -> Result<bool> {
    let Some(reason) = conflict(work_dir, stage, contract_id, dispute, proposed)? else {
        return Ok(false);
    };
    escalate_amendment_cap(work_dir, stage, &anyhow::anyhow!(reason));
    Ok(true)
}

/// [`write`] for the dispute's replacement, if it carries one; the file
/// written.
pub(super) fn write_proposed(
    proposed: Option<&Proposed<'_>>,
    site: &Site,
    stage: &Stage,
    contract_id: &str,
) -> Result<Option<String>> {
    proposed
        .map(|proposed| write(site, stage, contract_id, proposed.text))
        .transpose()
}

/// Write the replacement over the contract's file as the (amended) stage now
/// names it, keeping the file's permission bits, and return that file. Refuses
/// a symlink target, a symlinked parent and a deleted contract.
pub(super) fn write(site: &Site, stage: &Stage, contract_id: &str, text: &str) -> Result<String> {
    let Some(file) = contract_file(stage, contract_id) else {
        bail!(
            "the accepted amendment removed contract '{contract_id}'; its replacement was not \
             written"
        );
    };
    validate_relative(&file)?;
    let prefix = site
        .working_dir
        .strip_prefix(&site.worktree_root)
        .context("the stage's working directory is outside its worktree")?;
    let target = prefix.join(&file);
    if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
        let root = safe_open_dirfd(&site.worktree_root)?;
        safe_create_dir_all_in_workdir(root.as_raw_fd(), parent, 0o755)
            .with_context(|| format!("refusing to write {}", target.display()))?;
    }
    let mode = existing_mode(&site.worktree_root.join(&target));
    restore_no_follow(&site.worktree_root, &target, text.as_bytes(), mode)
        .with_context(|| format!("refusing to write {}", target.display()))?;
    #[cfg(test)]
    if FAIL_AFTER_WRITE.with(|fail| fail.replace(false)) {
        bail!("injected failure after the replacement write");
    }
    Ok(file)
}

/// The permission bits of the regular file at `path`; `restore_no_follow`
/// refuses anything else, so the default only applies to a missing file.
fn existing_mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map_or(DEFAULT_MODE, |metadata| {
            // Setuid, setgid and sticky bits are not carried onto the rewrite.
            metadata.permissions().mode() & 0o777
        })
}

#[cfg(test)]
#[path = "apply_contract_replacement_tests.rs"]
mod tests;
