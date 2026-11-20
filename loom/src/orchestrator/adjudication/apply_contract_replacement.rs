//! The replacement files an accepted contract ruling writes into the worktree.
//!
//! The bytes come from the admitted dispute record, never from the worktree, so
//! every apply attempt writes what the judge reviewed. Each frozen file's hash
//! must still be the one the dispute was filed against (or already the
//! replacement's, when an earlier attempt of the same verdict got further), so
//! a replay is idempotent and an accept never overwrites a file that changed
//! after filing.

use anyhow::{Context, Result};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::fs::safe_fs::{safe_create_dir_all_in_workdir, safe_open_dirfd};
use crate::fs::safe_replace::{open_parent, restore_no_follow};
use crate::models::dispute::FileReplacement;
use crate::stage_service::contracts::Site;
use crate::utils::sha256_hex;
use crate::verify::contracts::store::{validate_relative, FreezeRecord};

/// Mode of a replacement whose target file does not exist yet.
const DEFAULT_MODE: u32 = 0o644;

#[cfg(test)]
thread_local! {
    /// Test seam: fail once right after a write, as a crash before the next
    /// write or the re-freeze would.
    pub(in super::super) static FAIL_AFTER_WRITE: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// Why `replacement` must not be written, or `None` when the frozen file is
/// still what the dispute was filed against or already is the replacement.
/// Runs before the amendment and before any write.
pub(in super::super) fn conflict(
    record: &FreezeRecord,
    replacement: &FileReplacement,
    dispute: u32,
) -> Option<String> {
    let file = &replacement.file;
    let frozen = record
        .files
        .iter()
        .find(|frozen| frozen.path == *file)
        .map(|frozen| frozen.sha256.as_str());
    let written = sha256_hex(replacement.content.as_bytes());
    let unchanged = frozen
        .is_some_and(|hash| hash == written || Some(hash) == replacement.base_sha256.as_deref());
    (!unchanged).then(|| {
        format!(
            "the frozen `{file}` changed after dispute {dispute} was filed; its replacement was \
             not written"
        )
    })
}

/// Write `text` over the recorded frozen file `file` (relative to the stage's
/// working directory), keeping the file's permission bits. Refuses a symlink
/// target and a symlinked parent.
pub(in super::super) fn write(site: &Site, file: &str, text: &str) -> Result<()> {
    let target = target(site, file)?;
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
        anyhow::bail!("injected failure after the replacement write");
    }
    Ok(())
}

/// Refuse `file` before anything changes when [`write`] would refuse it as
/// the worktree stands: a symlinked parent directory, or a target that is not
/// a regular file. A missing target passes; `write` creates it. `write`
/// repeats every check, so a change in between is still refused there.
pub(in super::super) fn check_writable(site: &Site, file: &str) -> Result<()> {
    let target = target(site, file)?;
    let refused = || format!("refusing to write {}", target.display());
    match std::fs::symlink_metadata(site.worktree_root.join(&target)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(refused),
        Ok(_) => {}
    }
    // The no-follow walk `write` takes, so the stat below names the entry it
    // would replace.
    open_parent(&site.worktree_root, &target).with_context(refused)?;
    let metadata =
        std::fs::symlink_metadata(site.worktree_root.join(&target)).with_context(refused)?;
    if !metadata.is_file() {
        anyhow::bail!("{}: the target is not a regular file", refused());
    }
    Ok(())
}

/// `file` (relative to the stage's working directory) relative to the
/// worktree root.
fn target(site: &Site, file: &str) -> Result<PathBuf> {
    validate_relative(file)?;
    let prefix = site
        .working_dir
        .strip_prefix(&site.worktree_root)
        .context("the stage's working directory is outside its worktree")?;
    Ok(prefix.join(file))
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
