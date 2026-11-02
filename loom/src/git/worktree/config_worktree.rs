//! The `config.worktree` file of a linked worktree's administrative directory.
//!
//! When the repository's common configuration sets `extensions.worktreeConfig`,
//! git also reads `<common dir>/worktrees/<name>/config.worktree` for that
//! worktree. A session's sandbox may write the git common directory, so an
//! agent can put `filter.<name>.clean` (or `core.fsmonitor`, `core.pager`,
//! `include.path`) there and have git run a command the next time the daemon
//! runs it in that worktree. Two layers close this: the session capsule denies
//! writes to its own worktree's `config.worktree` and to every other
//! administrative directory present when it is built ([`partition_admin_dirs`]
//! splits them), and a pinned [`super::WorktreeGit`] never lets git open the
//! file: git runs with a private git directory holding a copy validated by
//! [`snapshot_worktree_config`] (see `scratch_admin`), which also covers an
//! administrative directory registered after a session spawned.

use anyhow::{bail, ensure, Context, Result};
use std::ffi::OsStr;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use super::pinned::common_dir;
use super::scratch_admin::open_regular;
use crate::git::runner::run_git_with_env;

pub(super) const CONFIG_WORKTREE: &str = "config.worktree";

/// Lowercase spelling searched for in the common configuration.
const WORKTREE_CONFIG_NAME: &[u8] = b"worktreeconfig";

/// Git writes a few lines here; more than this is not a file it wrote.
const MAX_CONFIG_WORKTREE_BYTES: u64 = 64 * 1024;

/// A git directory that cannot exist (`/dev/null` is not a directory).
const NO_REPOSITORY: &str = "/dev/null/loom-no-repository";

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
    let workspace = crate::fs::work_dir::WorkDir::new(repo_root)?;
    if !repo_root.join(".git").exists()
        && !super::main_identity::main_identity_exists(workspace.root())
    {
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
/// The listing is taken at spawn. A worktree registered later gets no deny:
/// its `HEAD`, `index`, `gitdir`, `logs/` and `config.worktree` stay writable to
/// a session spawned before it, and pinned git never reads that file
/// (`scratch_admin`).
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

/// Whether the common configuration enables `extensions.worktreeConfig`, read
/// the way git reads its repository format: that one file, no includes. A file
/// that never spells the variable name needs no git process, since a config
/// variable name has no escape syntax. A session cannot change the answer:
/// every capsule denies `<common dir>/config`.
pub(super) fn worktree_config_enabled(repo_root: &Path, common: &Path) -> Result<bool> {
    let file = common.join("config");
    let bytes = std::fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?;
    let mentioned = bytes
        .windows(WORKTREE_CONFIG_NAME.len())
        .any(|window| window.eq_ignore_ascii_case(WORKTREE_CONFIG_NAME));
    if !mentioned {
        return Ok(false);
    }
    let path = file
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", file.display()))?;
    let output = read_config_file(
        &[
            "config",
            "--file",
            path,
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

/// Copy `<admin>/config.worktree` to `into` once and validate the copy, the
/// only version git will read. An absent file or an empty one leaves no copy.
/// The keys are listed as data with `git config --file`, which runs nothing
/// and follows no include; any key outside [`GIT_WRITTEN_KEYS`], or a file git
/// cannot parse, is an error. It is read through the descriptor `admin`, opened on
/// `admin_path`, and anything but a regular file there is refused, a link above
/// all: git reads this copy and never that path, so following a link would only
/// copy a file of the session's choosing, one its sandbox hides, to where the
/// session can see what git makes of it.
pub(super) fn snapshot_worktree_config(
    repo_root: &Path,
    admin: &OwnedFd,
    admin_path: &Path,
    into: &Path,
) -> Result<()> {
    let file = admin_path.join(CONFIG_WORKTREE);
    let Some(bytes) = read_regular(admin, admin_path)? else {
        return Ok(());
    };
    if bytes.is_empty() {
        return Ok(());
    }
    let copy = into.join(CONFIG_WORKTREE);
    std::fs::write(&copy, bytes).with_context(|| format!("cannot write {}", copy.display()))?;
    let path = copy
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", copy.display()))?;
    let listed = read_config_file(
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

/// The bytes of `config.worktree` in `admin`, `None` when absent.
fn read_regular(admin: &OwnedFd, admin_path: &Path) -> Result<Option<Vec<u8>>> {
    let file = admin_path.join(CONFIG_WORKTREE);
    let context = || format!("cannot read {}", file.display());
    let Some(handle) = open_regular(admin, admin_path, CONFIG_WORKTREE)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    handle
        .take(MAX_CONFIG_WORKTREE_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(context)?;
    ensure!(
        bytes.len() as u64 <= MAX_CONFIG_WORKTREE_BYTES,
        "loom will not run git in the worktree: {} is over {MAX_CONFIG_WORKTREE_BYTES} bytes",
        file.display()
    );
    Ok(Some(bytes))
}

/// `git config --file ..` under a `GIT_DIR` that cannot exist: git then finds
/// no repository, so it loads no repository configuration for itself.
fn read_config_file(args: &[&str], repo_root: &Path) -> Result<std::process::Output> {
    run_git_with_env(args, &[("GIT_DIR", OsStr::new(NO_REPOSITORY))], repo_root)
}

#[cfg(test)]
#[path = "config_worktree_tests.rs"]
mod tests;
