//! Merge bases and change listings for the verification gates.
//!
//! Every call runs read-only through the caller's [`WorktreeGit`] (pinned
//! where the caller pins): `--no-optional-locks` keeps the commands that honour
//! it from writing a refreshed index back, and `core.fsmonitor=false` keeps git
//! from running an fsmonitor command named in a configuration the stage can
//! edit. `git diff` against the working tree refreshes and writes the index
//! whatever `--no-optional-locks` says, so it reads a private copy of the index
//! instead ([`PrivateIndex`]). Listings use `-z`, read stdout untrimmed and
//! require every path to be UTF-8, so a name with leading or trailing spaces is
//! listed as it is spelled.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tempfile::TempDir;

use crate::fs::safe_read::open_regular_no_follow;
use crate::git::worktree::{is_worktree_scaffold_path, WorktreeGit};
use crate::verify::tool_artifacts::is_tool_artifact;

/// Global options for every git call here: no index refresh written back,
/// and no fsmonitor command taken from a config the stage can edit.
const READ_ONLY: [&str; 3] = ["--no-optional-locks", "-c", "core.fsmonitor=false"];

/// A `git diff --name-only` query.
pub(crate) struct DiffNames<'a> {
    /// The commit the diff starts from.
    pub from: &'a str,
    /// The commit the diff ends at, or `None` for the working tree.
    pub to: Option<&'a str>,
    /// A `--diff-filter` value, e.g. `"A"` or `"ACMRT"`.
    pub filter: Option<&'a str>,
    /// Pass `--ignore-submodules=none`. The runner (`crate::git::runner`)
    /// drops it from every `git diff` in favour of `--ignore-submodules=dirty`,
    /// which still lists a gitlink whose commit moved but starts no git inside
    /// it.
    pub all_submodules: bool,
    /// Pathspecs after `--`; empty lists the whole tree.
    pub pathspecs: &'a [&'a str],
}

/// The merge base of `a` and `b`. Fails when git fails, prints no id, or
/// prints anything but one 40- or 64-digit hexadecimal object id.
pub(crate) fn merge_base(git: &WorktreeGit, a: &str, b: &str) -> Result<String> {
    let output = read_only(git, &["merge-base", a, b])?;
    let text = String::from_utf8(output).context("git merge-base printed non-UTF-8 output")?;
    let id = text.strip_suffix('\n').unwrap_or(&text);
    if id.is_empty() {
        bail!("no merge base between '{a}' and '{b}'");
    }
    let hex = id.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !hex || !matches!(id.len(), 40 | 64) {
        bail!("git merge-base {a} {b} printed '{id}', not an object id");
    }
    Ok(id.to_string())
}

/// The paths `git diff --name-only` lists for `query`, with neither rename
/// detection (a move lists both paths) nor an external diff driver.
pub(crate) fn changed_names(git: &WorktreeGit, query: &DiffNames<'_>) -> Result<BTreeSet<String>> {
    let filter = query.filter.map(|value| format!("--diff-filter={value}"));
    let mut args = vec!["diff", "-z", "--name-only", "--no-renames", "--no-ext-diff"];
    args.extend(filter.as_deref());
    if query.all_submodules {
        args.push("--ignore-submodules=none");
    }
    args.push(query.from);
    args.extend(query.to);
    args.push("--");
    args.extend(query.pathspecs);
    if query.to.is_some() {
        return nul_paths(&read_only(git, &args)?);
    }
    let index = PrivateIndex::copy_of(git)?;
    let mut argv = vec!["-c", "core.splitIndex=false"];
    argv.extend(args);
    nul_paths(&read_only_with(
        git,
        &argv,
        &[("GIT_INDEX_FILE", index.path().as_os_str())],
    )?)
}

/// The untracked files git does not ignore under `pathspecs` (the whole tree
/// when empty).
pub(crate) fn untracked_names(git: &WorktreeGit, pathspecs: &[&str]) -> Result<BTreeSet<String>> {
    let mut args = vec!["ls-files", "-z", "--others", "--exclude-standard", "--"];
    args.extend(pathspecs);
    nul_paths(&read_only(git, &args)?)
}

/// Every worktree-relative path changed since `base`: committed on the
/// branch, changed in the index or working tree, or untracked and not
/// ignored. Untracked sandbox artifacts ([`is_tool_artifact`]) and worktree
/// scaffolding ([`is_worktree_scaffold_path`]) are left out. `filter` is a
/// `--diff-filter` value limiting the tracked changes (untracked files are
/// listed either way); `None` lists every kind.
pub(crate) fn changes_since(
    git: &WorktreeGit,
    base: &str,
    filter: Option<&str>,
) -> Result<BTreeSet<String>> {
    let worktree = git.work_tree();
    let query = DiffNames {
        from: base,
        to: None,
        filter,
        all_submodules: false,
        pathspecs: &[],
    };
    let mut paths = changed_names(git, &query)?;
    paths.extend(
        untracked_names(git, &[])?
            .into_iter()
            .filter(|path| !is_tool_artifact(worktree, path)),
    );
    paths.retain(|path| !is_worktree_scaffold_path(path));
    Ok(paths)
}

/// A copy of `git`'s index in a private directory, for a command that
/// refreshes the index and writes it back: the write lands in the copy, and
/// `core.splitIndex=false` keeps it from writing a shared index file into the
/// git directory. The copy keeps the index's modification time, which git
/// reads to tell a racily clean entry from a changed one. A missing index
/// stays missing, which git reads as an empty one.
struct PrivateIndex {
    dir: TempDir,
}

impl PrivateIndex {
    fn copy_of(git: &WorktreeGit) -> Result<Self> {
        let git_dir = match git.git_dir_path() {
            Some(dir) => dir.to_path_buf(),
            None => absolute_git_dir(git)?,
        };
        let dir = tempfile::tempdir().context("cannot create a private index directory")?;
        let index = Self { dir };
        let source = open_regular_no_follow(&git_dir, "index", libc::O_RDONLY)
            .with_context(|| format!("cannot open the index in {}", git_dir.display()))?;
        if let Some(mut source) = source {
            let modified = source.metadata()?.modified()?;
            let mut copy = File::create(index.path()).context("cannot create the private index")?;
            std::io::copy(&mut source, &mut copy).context("cannot copy the index")?;
            copy.set_modified(modified)
                .context("cannot keep the index's modification time")?;
        }
        Ok(index)
    }

    fn path(&self) -> PathBuf {
        self.dir.path().join("index")
    }
}

/// The git directory of an unpinned `git`, as git discovers it.
fn absolute_git_dir(git: &WorktreeGit) -> Result<PathBuf> {
    let output = read_only(git, &["rev-parse", "--absolute-git-dir"])?;
    let text = String::from_utf8(output).context("git printed a non-UTF-8 git directory")?;
    let dir = Path::new(text.strip_suffix('\n').unwrap_or(&text));
    if !dir.is_absolute() {
        bail!(
            "git printed '{}', not an absolute git directory",
            dir.display()
        );
    }
    Ok(dir.to_path_buf())
}

/// `git <READ_ONLY> <args>` through `git`; its untrimmed stdout on success.
pub(crate) fn read_only(git: &WorktreeGit, args: &[&str]) -> Result<Vec<u8>> {
    read_only_with(git, args, &[])
}

/// [`read_only`] with `env` added to git's environment.
fn read_only_with(git: &WorktreeGit, args: &[&str], env: &[(&str, &OsStr)]) -> Result<Vec<u8>> {
    let argv: Vec<&str> = READ_ONLY.iter().chain(args).copied().collect();
    let output = if env.is_empty() {
        git.run(&argv)?
    } else {
        git.read_isolated(&argv, env)?
    };
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            git.work_tree().display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

/// The NUL-separated paths git listed, each required to be UTF-8.
fn nul_paths(listed: &[u8]) -> Result<BTreeSet<String>> {
    listed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .context("git returned a non-UTF-8 path")
                .map(str::to_owned)
        })
        .collect()
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
