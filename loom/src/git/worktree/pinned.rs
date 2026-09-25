//! Git run in a stage worktree by a process outside the stage's sandbox.
//!
//! A linked worktree's `.git` is a file naming its git directory, and the
//! agent working in the worktree can rewrite it to name a directory of its
//! own. That directory's configuration could define `filter.<name>.clean`,
//! which `git diff` and `git status` run when they re-hash a file whose stat
//! data changed, or any other command git takes from configuration. Git that
//! the daemon starts with default discovery would run it outside the sandbox.
//!
//! A pinned [`WorktreeGit`] never reads that file. It sets `GIT_DIR` and
//! `GIT_WORK_TREE` (what `--git-dir` and `--work-tree` set) to the worktree's
//! administrative directory in the main repository and to the worktree, and
//! `GIT_COMMON_DIR` to the main repository's common directory, which also
//! makes git ignore the administrative directory's own `commondir` file.
//! Both directories come from the main repository's records. Repository
//! configuration then comes from the main repository's `config` alone, beside
//! the user's global and system files, so a filter defined there (git-lfs)
//! keeps working. One exception: a repository that enables
//! `extensions.worktreeConfig` also has git read `config.worktree` from the
//! administrative directory, which sits beside the worktree's index.

use anyhow::{ensure, Context, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::fs::safe_read::read_to_string_bounded;
use crate::fs::work_dir::WorkDir;
use crate::git::runner::{run_git, run_git_checked, run_git_with_env};

/// Largest `gitdir` or `commondir` file read from an administrative directory.
const MAX_ADMIN_FILE_BYTES: usize = 4096;

/// How loom runs git in one worktree.
#[derive(Debug, Clone)]
pub struct WorktreeGit {
    work_tree: PathBuf,
    pin: Option<Pin>,
}

/// The directories a pinned worktree's git runs with.
#[derive(Debug, Clone)]
struct Pin {
    git_dir: PathBuf,
    common_dir: PathBuf,
}

impl WorktreeGit {
    /// Git in `worktree` as the worktree's own `.git` directs it. Only for a
    /// directory that is no stage worktree, or for a process that already
    /// runs with the privileges of whoever controls the worktree: a stage
    /// agent's own CLI inside its sandbox.
    pub fn discovered(worktree: &Path) -> Self {
        Self {
            work_tree: worktree.to_path_buf(),
            pin: None,
        }
    }

    /// Git in `worktree` pinned to its administrative directory in the
    /// repository at `repo_root` (see the module docs). Fails, running
    /// nothing in the worktree, unless exactly one entry of
    /// `<common dir>/worktrees/` is a directory (not a symlink) whose
    /// `gitdir` file names `<worktree>/.git` and whose `commondir` file
    /// resolves to that common directory.
    pub fn pinned(repo_root: &Path, worktree: &Path) -> Result<Self> {
        let work_tree = worktree
            .canonicalize()
            .with_context(|| format!("cannot resolve the worktree {}", worktree.display()))?;
        let common_dir = common_dir(repo_root)?;
        let git_dir = registered_admin_dir(&common_dir, &work_tree)?;
        Ok(Self {
            work_tree,
            pin: Some(Pin {
                git_dir,
                common_dir,
            }),
        })
    }

    /// [`Self::pinned`] in the repository that holds the loom state
    /// directory `work_dir`.
    pub fn pinned_in_project_of(work_dir: &Path, worktree: &Path) -> Result<Self> {
        let workspace = WorkDir::new(work_dir)?;
        let repo_root = workspace
            .repo_root()
            .context("cannot resolve the repository root of the state directory")?;
        Self::pinned(repo_root, worktree)
    }

    /// The worktree git runs in.
    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }

    /// `git <args>` in the worktree, through [`run_git`].
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        let Some(pin) = &self.pin else {
            return run_git(args, &self.work_tree);
        };
        let env: [(&str, &OsStr); 3] = [
            ("GIT_DIR", pin.git_dir.as_os_str()),
            ("GIT_WORK_TREE", self.work_tree.as_os_str()),
            ("GIT_COMMON_DIR", pin.common_dir.as_os_str()),
        ];
        run_git_with_env(args, &env, &self.work_tree)
    }
}

/// The canonical common directory of the repository at `repo_root`.
fn common_dir(repo_root: &Path) -> Result<PathBuf> {
    let listed = run_git_checked(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        repo_root,
    )?;
    Path::new(&listed)
        .canonicalize()
        .with_context(|| format!("cannot resolve the git common directory {listed}"))
}

/// The one entry of `<common_dir>/worktrees/` registered for `work_tree`.
fn registered_admin_dir(common_dir: &Path, work_tree: &Path) -> Result<PathBuf> {
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
fn names_work_tree(admin: &Path, work_tree: &Path) -> bool {
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

#[cfg(test)]
#[path = "pinned_tests.rs"]
mod tests;
