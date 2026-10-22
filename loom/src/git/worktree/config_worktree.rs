//! The `config.worktree` file of a linked worktree's administrative directory.
//!
//! When the repository's common configuration sets `extensions.worktreeConfig`,
//! git also reads `<common dir>/worktrees/<name>/config.worktree` for that
//! worktree. A session's sandbox may write the git common directory, so an
//! agent can put `filter.<name>.clean` (or `core.fsmonitor`, `core.pager`,
//! `include.path`) there and have git run a command the next time the daemon
//! runs it in that worktree. Two layers close this: the session capsule denies
//! writes to its own worktree's `config.worktree` and to every other
//! administrative directory whole ([`partition_admin_dirs`] splits them), and
//! a pinned [`super::WorktreeGit`] refuses to run git while the file holds a
//! key outside [`GIT_WRITTEN_KEYS`] ([`check_worktree_config`]).

use anyhow::{bail, ensure, Context, Result};
use std::path::{Path, PathBuf};

use super::pinned::common_dir;
use crate::git::runner::run_git;

/// The settings git itself writes to `config.worktree` when it creates a
/// worktree (`git worktree add` copies the main worktree's sparse-checkout
/// settings there), lowercased. None of them runs a program.
const GIT_WRITTEN_KEYS: [&str; 3] = [
    "core.sparsecheckout",
    "core.sparsecheckoutcone",
    "index.sparse",
];

/// The administrative directories under `<common dir>/worktrees/` of the
/// repository at `repo_root`: real directories only, no symlinks. A symlinked
/// registry entry is therefore never listed, so it gets no deny. A directory
/// with no `.git` has no worktrees.
pub fn worktree_admin_dirs(repo_root: &Path) -> Result<Vec<PathBuf>> {
    if !repo_root.join(".git").exists() {
        return Ok(Vec::new());
    }
    let registry = common_dir(repo_root)?.join("worktrees");
    let entries = match std::fs::read_dir(&registry) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot list {}", registry.display()))
        }
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("cannot list {}", registry.display()))?;
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// The administrative directories of `repo_root`'s worktrees, split by the
/// session that runs at `worktree`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdminDirs {
    /// The directory whose `gitdir` records `<worktree>/.git`; `None` for a
    /// session in the checkout.
    pub own: Option<PathBuf>,
    /// Every other administrative directory present now.
    pub siblings: Vec<PathBuf>,
}

/// Split [`worktree_admin_dirs`] into the one registered for `worktree` and
/// the rest; with `worktree` `None` (a session in the checkout) every
/// directory is a sibling. With no administrative directory at all there is
/// nothing to deny, and `worktree` is never touched, so it need not exist:
/// pinned git refuses an unregistered worktree on its own.
///
/// Once the registry lists at least one directory, a `worktree` that no
/// directory names, or that two directories name, is an error: the capsule
/// cannot tell which directory is the session's own, so the spawn is refused.
/// That fails closed for a directory on disk that no registry entry claims,
/// the adjudication judge (`judge_cwd`) and the merge resolver
/// (`merge_resolver_worktree` in `orchestrator/terminal/backend.rs`) included,
/// since both check only `is_dir()`.
///
/// The listing is taken at spawn. A worktree registered later is covered
/// only for its `config.worktree` content (`WorktreeGit::run`'s
/// `check_worktree_config`); its `HEAD`, `index`, `gitdir` and `logs/` stay
/// writable to a session spawned before it.
pub fn partition_admin_dirs(repo_root: &Path, worktree: Option<&Path>) -> Result<AdminDirs> {
    let dirs = worktree_admin_dirs(repo_root)?;
    if dirs.is_empty() {
        return Ok(AdminDirs::default());
    }
    let Some(worktree) = worktree else {
        return Ok(AdminDirs {
            own: None,
            siblings: dirs,
        });
    };
    // Every listed directory is an entry of `<common dir>/worktrees`.
    let registry = dirs[0].parent().map(Path::to_path_buf).unwrap_or_default();
    let canonical = worktree
        .canonicalize()
        .with_context(|| format!("cannot resolve the worktree {}", worktree.display()))?;
    let (owned, siblings): (Vec<PathBuf>, Vec<PathBuf>) = dirs
        .into_iter()
        .partition(|dir| super::pinned::names_work_tree(dir, &canonical));
    ensure!(
        owned.len() < 2,
        "more than one worktree registered in {} names {}",
        registry.display(),
        worktree.display()
    );
    let own = owned.into_iter().next().with_context(|| {
        format!(
            "{} is not a registered worktree: no directory in {} names it",
            worktree.display(),
            registry.display()
        )
    })?;
    Ok(AdminDirs {
        own: Some(own),
        siblings,
    })
}

/// Refuse, unless git ignores or only git wrote `<admin>/config.worktree`.
///
/// Nothing is read when the file is absent, is an empty regular file (the
/// placeholder the sandbox leaves for a denied path; a symlink never takes this
/// shortcut), or the repository at `repo_root` leaves
/// `extensions.worktreeConfig` off (git ignores the file then). The
/// file's keys are listed as data with `git config --file`, which runs
/// nothing, in the main repository; any key outside [`GIT_WRITTEN_KEYS`], or
/// a file git cannot parse, is an error.
pub(super) fn check_worktree_config(repo_root: &Path, admin: &Path) -> Result<()> {
    let file = admin.join("config.worktree");
    match std::fs::symlink_metadata(&file) {
        Err(_) => return Ok(()),
        Ok(meta) if meta.is_file() && meta.len() == 0 => return Ok(()),
        Ok(_) => {}
    }
    if !worktree_config_enabled(repo_root)? {
        return Ok(());
    }
    let path = file
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", file.display()))?;
    let listed = run_git(
        &["config", "--file", path, "--name-only", "--list"],
        repo_root,
    )?;
    ensure!(
        listed.status.success(),
        "loom will not run git in the worktree: {} cannot be parsed ({})",
        file.display(),
        String::from_utf8_lossy(&listed.stderr).trim()
    );
    let listing = String::from_utf8_lossy(&listed.stdout);
    let offending: Vec<&str> = listing
        .lines()
        .filter(|key| !GIT_WRITTEN_KEYS.contains(&key.to_lowercase().as_str()))
        .collect();
    ensure!(
        offending.is_empty(),
        "loom will not run git in the worktree: {} sets {}, which git would act on",
        file.display(),
        offending.join(", ")
    );
    Ok(())
}

/// Whether the repository's common configuration enables
/// `extensions.worktreeConfig`.
fn worktree_config_enabled(repo_root: &Path) -> Result<bool> {
    let output = run_git(
        &[
            "config",
            "--type=bool",
            "--get",
            "extensions.worktreeConfig",
        ],
        repo_root,
    )?;
    match output.status.code() {
        Some(1) => Ok(false),
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout).trim() == "true"),
        _ => bail!(
            "cannot read the repository configuration: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

#[cfg(test)]
#[path = "config_worktree_tests.rs"]
mod tests;
