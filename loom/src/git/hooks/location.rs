//! Where git runs this repository's hooks from, and whether loom may write there.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::configured_hooks_path;
use crate::git::merge::control_paths::resolve_hooks_dir_prefix_checked;
use crate::git::runner::run_git_checked;

const DEFAULT_HOOKS_DIR: &str = ".git/hooks";

/// The directory git runs hooks from: `core.hooksPath`, which git resolves against
/// the checkout root when relative, else `.git/hooks`.
pub(super) fn active_hooks_dir(repo_root: &Path) -> PathBuf {
    match configured_hooks_path(repo_root) {
        Some(configured) => repo_root.join(configured),
        None => repo_root.join(DEFAULT_HOOKS_DIR),
    }
}

/// `active_hooks_dir` when loom may write its hooks there. The error says why
/// not, for the operator:
/// - outside the repository, which also covers `/dev/null`: other repositories may
///   share that directory;
/// - inside a directory git tracks: a hook loom wrote or moved aside would show as
///   the operator's own change, and committing it holds the target;
/// - a path loom cannot resolve safely, such as one through a symlink.
pub fn installable_hooks_dir(repo_root: &Path) -> Result<PathBuf> {
    let Some(configured) = configured_hooks_path(repo_root) else {
        return Ok(repo_root.join(DEFAULT_HOOKS_DIR));
    };
    let prefix = resolve_hooks_dir_prefix_checked(repo_root, Some(&configured), None, None)
        .with_context(|| format!("core.hooksPath is {configured}, which loom cannot resolve"))?;
    let Some(prefix) = prefix else {
        bail!(
            "core.hooksPath is {configured}, outside this repository, so git runs no hook from \
             .git/hooks and loom does not write to a directory other repositories may share"
        );
    };
    let prefix = prefix.trim_end_matches('/');
    if tracks_files_under(repo_root, prefix)? {
        bail!(
            "core.hooksPath is {configured}, a directory git tracks files in, so git runs no \
             hook from .git/hooks and loom does not change tracked content"
        );
    }
    Ok(repo_root.join(prefix))
}

/// The directory to install in. `None` when loom may not write to it but
/// `installed` says the hook is there already, which is how an operator serves a
/// shared `core.hooksPath`.
pub(super) fn hooks_dir_for_install(
    repo_root: &Path,
    installed: impl FnOnce() -> bool,
) -> Result<Option<PathBuf>> {
    match installable_hooks_dir(repo_root) {
        Ok(dir) => Ok(Some(dir)),
        Err(_) if installed() => Ok(None),
        Err(error) => Err(error),
    }
}

fn tracks_files_under(repo_root: &Path, prefix: &str) -> Result<bool> {
    let directory = if prefix.is_empty() { "." } else { prefix };
    let pathspec = format!(":(literal){directory}");
    let tracked = run_git_checked(&["ls-files", "--", &pathspec], repo_root)?;
    Ok(!tracked.is_empty())
}
