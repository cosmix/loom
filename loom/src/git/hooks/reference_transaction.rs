//! Installs loom's `reference-transaction` hook, which attests host moves of a
//! guarded target in the target guard's ledger and refuses a loom session's
//! move it cannot attest (see `loom-hooks/git-reference-transaction-hook.sh`).

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use super::{
    active_hooks_dir, has_installed_mode, hooks_dir_for_install, make_executable, path_occupied,
};
use crate::git::target_guard::HOOK_MARKER;

/// The hook script, embedded from `loom-hooks/git-reference-transaction-hook.sh`.
pub(super) const SCRIPT: &str =
    include_str!("../../../../loom-hooks/git-reference-transaction-hook.sh");

/// What [`install_reference_transaction_hook`] found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookInstall {
    /// Loom's current hook was written: there was no hook, or loom's was out
    /// of date.
    Installed,
    /// Loom's current hook was already in place.
    UpToDate,
    /// Another tool's hook is in place; it was left untouched.
    ForeignHookPresent,
}

/// Install loom's hook as `reference-transaction` in the directory git runs hooks
/// from in `repo_root`.
///
/// Refuses when `repo_root` has no `.git` directory, or when `core.hooksPath` names
/// a directory loom must not write to and the hook is not already there
/// ([`super::installable_hooks_dir`]). A hook without [`HOOK_MARKER`] belongs to
/// another tool and is never overwritten, nor is a symlink whose target is missing.
pub fn install_reference_transaction_hook(repo_root: &Path) -> Result<HookInstall> {
    if !repo_root.join(".git").is_dir() {
        bail!(
            "{} is not a git repository (no .git directory); run `loom init` to set one up",
            repo_root.display()
        );
    }
    let Some(hooks_dir) = hooks_dir_for_install(repo_root, || {
        is_reference_transaction_hook_installed(repo_root)
    })?
    else {
        return Ok(HookInstall::UpToDate);
    };
    fs::create_dir_all(&hooks_dir)
        .with_context(|| format!("Failed to create hooks directory: {}", hooks_dir.display()))?;
    let hook_path = hooks_dir.join("reference-transaction");
    if let Some(existing) = read_existing(&hook_path)? {
        if !has_marker(&existing) {
            return Ok(HookInstall::ForeignHookPresent);
        }
        if existing == SCRIPT.as_bytes() && has_installed_mode(&hook_path)? {
            return Ok(HookInstall::UpToDate);
        }
    } else if path_occupied(&hook_path)? {
        return Ok(HookInstall::ForeignHookPresent);
    }
    write_script(&hook_path)?;
    Ok(HookInstall::Installed)
}

/// Whether the hook git would run in `repo_root` is loom's.
pub fn is_reference_transaction_hook_installed(repo_root: &Path) -> bool {
    fs::read(active_hooks_dir(repo_root).join("reference-transaction"))
        .is_ok_and(|content| has_marker(&content))
}

/// Whether anything sits at the path of the hook git would run in `repo_root`, a
/// symlink with a missing target included. An unreadable path counts as occupied,
/// since nothing may be written over it.
pub fn reference_transaction_hook_occupied(repo_root: &Path) -> bool {
    path_occupied(&active_hooks_dir(repo_root).join("reference-transaction")).unwrap_or(true)
}

/// Whether a hook's bytes (a foreign hook need not be text) carry [`HOOK_MARKER`].
fn has_marker(content: &[u8]) -> bool {
    let marker = HOOK_MARKER.as_bytes();
    content.windows(marker.len()).any(|window| window == marker)
}

/// The hook's contents; `None` when there is no hook.
fn read_existing(hook_path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(hook_path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to read hook: {}", hook_path.display()))
        }
    }
}

/// Whether the hook has the mode [`write_script`] gives it (git skips a hook
/// without the executable bit).
/// Write [`SCRIPT`] beside `hook_path`, make it executable, then rename it
/// into place: git never runs a partly written or non-executable hook. A
/// failure removes the staged file.
pub(super) fn write_script(hook_path: &Path) -> Result<()> {
    let staged = hook_path.with_extension("loom-new");
    let installed = fs::write(&staged, SCRIPT)
        .with_context(|| format!("Failed to write hook: {}", staged.display()))
        .and_then(|()| make_executable(&staged))
        .and_then(|()| {
            fs::rename(&staged, hook_path)
                .with_context(|| format!("Failed to install hook: {}", hook_path.display()))
        });
    if installed.is_err() {
        // Best effort: the error that matters is the one in `installed`.
        let _ = fs::remove_file(&staged);
    }
    installed
}
