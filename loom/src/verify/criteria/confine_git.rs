//! Read-only Git metadata for a command confined to a linked worktree. The
//! worktree's `.git` is a file naming its administrative directory inside the
//! main repository's common directory; without those directories every `git`
//! command fails "not a git repository". The common directory's hooks, other
//! worktrees' administrative directories, the main checkout's index and
//! FETCH_HEAD stay hidden. Only files are parsed here: running `git` would
//! execute commands the repository's config names.
//!
//! Whoever writes the tree writes its `.git` file, so that file only proposes
//! a directory. The repository it must belong to comes from where loom placed
//! the tree, `<repository>/.worktrees/<stage>`. The proposal is bound only when
//! it lies in that repository's `.git/worktrees/`, its `commondir` names that
//! `.git`, and its `gitdir` names the tree's `.git` back. Anything else binds
//! nothing: Git fails inside the boundary and the rest of the command runs.
use anyhow::{ensure, Context, Result};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::NetworkAccess;
use crate::fs::safe_read::read_to_string_bounded;

/// Entries of the common directory a read-only Git command needs.
const COMMON_ENTRIES: [&str; 8] = [
    "config",
    "objects",
    "refs",
    "packed-refs",
    "reftable",
    "info",
    "HEAD",
    "shallow",
];

/// Bound on a `.git`, `commondir` or `gitdir` file, each naming one path.
const MAX_POINTER_BYTES: usize = 4 * 1024;

/// The Git directories a command confined to a tree uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::verify::criteria) struct GitDirs {
    /// The tree's own Git directory: `HEAD` and the index.
    pub(in crate::verify::criteria) admin: PathBuf,
    /// The repository's shared directory: refs and objects.
    pub(in crate::verify::criteria) common: PathBuf,
    /// Canonical paths outside the tree to expose read-only; empty when the
    /// tree holds its own Git directory.
    pub(in crate::verify::criteria) roots: Vec<PathBuf>,
}

/// The Git directories of the canonical `tree` for a command denied the
/// network. `None` under provisioning, whose network access could carry the
/// repository's objects and config away, for a tree without `.git`, and for a
/// linked worktree that fails verification.
pub(super) fn git_dirs(tree: &Path, network: NetworkAccess) -> Option<GitDirs> {
    if network != NetworkAccess::Denied {
        return None;
    }
    let dot_git = tree.join(".git");
    let linked = match dot_git.symlink_metadata() {
        // A main checkout: its Git directory lies inside the tree.
        Ok(metadata) if metadata.is_dir() => {
            return Some(GitDirs {
                admin: dot_git.clone(),
                common: dot_git,
                roots: Vec::new(),
            });
        }
        Ok(metadata) if metadata.is_file() => loom_repository(tree)
            .context("the tree is not at <repository>/.worktrees/<stage>")
            .and_then(|repository| linked_dirs(tree, repository)),
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Ok(_) => Err(anyhow::anyhow!(
            "{} is not a regular file",
            dot_git.display()
        )),
        Err(error) => Err(error).with_context(|| format!("inspect {}", dot_git.display())),
    };
    linked
        .inspect_err(|error| {
            tracing::warn!(
                "confined command in {} gets no Git metadata: {error:#}",
                tree.display()
            );
        })
        .ok()
}

/// The main checkout loom keeps `tree` for. Loom places every worktree at
/// `<repository>/.worktrees/<stage>` (`WorktreePath::worktree_path`), so the
/// path loom chose names the repository; the tree's content never does.
fn loom_repository(tree: &Path) -> Option<&Path> {
    let parent = tree.parent()?;
    if parent.file_name()? != ".worktrees" {
        return None;
    }
    parent.parent()
}

/// The administrative directory `tree`'s `.git` file names, verified from both
/// ends against `repository`'s Git directory, and the existing
/// [`COMMON_ENTRIES`] that resolve to themselves: an entry that is a symlink
/// would expose whatever it names.
fn linked_dirs(tree: &Path, repository: &Path) -> Result<GitDirs> {
    let tree = tree.canonicalize().context("resolve the confined tree")?;
    let main = repository.join(".git");
    ensure!(
        main.symlink_metadata()
            .is_ok_and(|metadata| metadata.is_dir()),
        "{} is not a Git directory",
        main.display()
    );
    let common = main.canonicalize()?;
    let admin = named_path(&tree, ".git", "gitdir:")?;
    ensure!(
        admin.parent() == Some(common.join("worktrees").as_path()),
        "{} is not a worktree directory of {}",
        admin.display(),
        common.display()
    );
    ensure!(
        named_path(&admin, "commondir", "")? == common,
        "{} names another common directory",
        admin.display()
    );
    ensure!(
        named_path(&admin, "gitdir", "")? == tree.join(".git"),
        "{} belongs to another worktree",
        admin.display()
    );
    let mut roots = vec![admin.clone()];
    roots.extend(
        COMMON_ENTRIES
            .iter()
            .map(|entry| common.join(entry))
            .filter(|path| {
                path.canonicalize()
                    .is_ok_and(|canonical| canonical == *path)
            }),
    );
    Ok(GitDirs {
        admin,
        common,
        roots,
    })
}

/// The canonical path the first line of `dir/file` names after `prefix`,
/// relative to `dir`. The file is read only as a bounded regular file: a FIFO
/// or a symlink there is refused without blocking.
fn named_path(dir: &Path, file: &str, prefix: &str) -> Result<PathBuf> {
    let pointer = dir.join(file);
    let content = read_to_string_bounded(dir, Path::new(file), MAX_POINTER_BYTES)?;
    let named = content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix(prefix))
        .map(str::trim)
        .filter(|named| !named.is_empty())
        .with_context(|| format!("{} names no path", pointer.display()))?;
    dir.join(named)
        .canonicalize()
        .with_context(|| format!("resolve the path {} names", pointer.display()))
}

#[cfg(test)]
#[path = "confine_git_tests.rs"]
mod tests;
