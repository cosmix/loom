//! Which administrative directory the main repository's records assign to a
//! worktree, read without following a link a session planted.

use anyhow::{ensure, Context, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::fs::safe_read::read_to_string_bounded;

/// Largest `gitdir` or `commondir` file read from an administrative directory.
const MAX_ADMIN_FILE_BYTES: usize = 4096;

pub(in crate::git::worktree) fn registered_checkout(worktree: &Path) -> Result<(PathBuf, PathBuf)> {
    let pointer = read_to_string_bounded(worktree, Path::new(".git"), MAX_ADMIN_FILE_BYTES)?;
    let path = pointer
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("gitdir: ")
        .context("invalid checkout gitdir pointer")?;
    ensure!(!path.is_empty(), "empty checkout gitdir pointer");
    let admin = worktree
        .join(path)
        .canonicalize()
        .context("cannot resolve checkout gitdir")?;
    let registry = admin.parent().context("checkout gitdir has no registry")?;
    ensure!(
        registry.file_name() == Some(OsStr::new("worktrees")),
        "checkout gitdir is outside a worktree registry"
    );
    let common = registry
        .parent()
        .context("worktree registry has no common directory")?
        .canonicalize()?;
    ensure!(
        names_work_tree(&admin, worktree),
        "checkout is not registered in its gitdir"
    );
    ensure!(
        recorded_path(&admin, "commondir")?.canonicalize()? == common,
        "checkout gitdir has a different common directory"
    );
    ensure!(
        registered_admin_dir(&common, worktree)? == admin,
        "checkout registration is ambiguous"
    );
    Ok((admin, common))
}

/// The canonical common directory of the repository at `repo_root`.
pub(in crate::git::worktree) fn common_dir(repo_root: &Path) -> Result<PathBuf> {
    let root = repo_root.canonicalize()?;
    Ok(crate::git::worktree::main_identity::identity_for(&root)?.1)
}

/// The one entry of `<common_dir>/worktrees/` registered for `work_tree`.
pub(super) fn registered_admin_dir(common_dir: &Path, work_tree: &Path) -> Result<PathBuf> {
    let registry = common_dir.join("worktrees");
    let listing = || format!("cannot list {}", registry.display());
    let mut found: Option<PathBuf> = None;
    for entry in std::fs::read_dir(&registry).with_context(listing)? {
        let entry = entry.with_context(listing)?;
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_dir && names_work_tree(&entry.path(), work_tree) {
            ensure!(
                found.replace(entry.path()).is_none(),
                "more than one worktree registered in {} names {}",
                registry.display(),
                work_tree.display()
            );
        }
    }
    let admin = found.with_context(|| {
        format!(
            "{} is not a registered worktree of the repository at {}",
            work_tree.display(),
            common_dir.display()
        )
    })?;
    let recorded_common = recorded_path(&admin, "commondir")?.canonicalize().ok();
    ensure!(
        recorded_common.as_deref() == Some(common_dir),
        "the git directory {} does not belong to the repository at {}",
        admin.display(),
        common_dir.display()
    );
    Ok(admin)
}

/// Whether the administrative directory `admin`'s `gitdir` file names
/// `<work_tree>/.git`.
pub(in crate::git::worktree) fn names_work_tree(admin: &Path, work_tree: &Path) -> bool {
    let Ok(dot_git) = recorded_path(admin, "gitdir") else {
        return false;
    };
    dot_git.file_name() == Some(OsStr::new(".git"))
        && dot_git
            .parent()
            .and_then(|dir| dir.canonicalize().ok())
            .is_some_and(|dir| dir == work_tree)
}

/// The path the file `name` in `admin` records on its one line, resolved
/// against `admin` when relative, as git resolves it. The file is read
/// without following a symlink.
fn recorded_path(admin: &Path, name: &str) -> Result<PathBuf> {
    let content = read_to_string_bounded(admin, Path::new(name), MAX_ADMIN_FILE_BYTES)?;
    let recorded = content.trim_end_matches(['\n', '\r']);
    ensure!(!recorded.is_empty(), "{}/{name} is empty", admin.display());
    Ok(admin.join(recorded))
}
