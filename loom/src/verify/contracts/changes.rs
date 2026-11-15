//! What a contract session changed and which files the freeze covers, read
//! from the stage worktree with read-only git (DESIGN D8 step 1).
//!
//! Paths come back relative to the stage's working directory, the frame
//! contract `file`s and `harness` globs are written in. A path outside it
//! keeps its `../` prefix, so it can never pass as a contract or harness file.
//!
//! Git runs through the caller's [`WorktreeGit`]: the daemon pins it to the
//! stage's registered git directory, so nothing the stage wrote into the
//! worktree's `.git` file decides which configuration git reads.

use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use super::special_walk::{escaped, special_paths};
use super::{harness_matches, store};
use crate::fs::work_dir::WorkDir;
use crate::git::changes::{changed_names, merge_base, read_only, DiffNames};
use crate::git::status_entries::{parse_porcelain_v1_z, PORCELAIN_V1_Z};
use crate::git::worktree::{is_worktree_scaffold_path, WorktreeGit};
use crate::verify::tool_artifacts::is_tool_artifact;

/// The commit the stage branch forked from: the merge base of `HEAD` and the
/// merge target configured in the state directory `work_dir`, a default
/// target resolved in the repository that holds `work_dir`.
pub fn stage_base(repo: &WorktreeGit, work_dir: &Path) -> Result<String> {
    let workspace = WorkDir::new(work_dir)?;
    let project = workspace
        .repo_root()
        .context("cannot resolve the repository root of the state directory")?;
    let target = crate::fs::resolve_target_branch_from_config(work_dir, project)?;
    merge_base(repo, &target, "HEAD")
        .with_context(|| format!("no merge base between HEAD and '{target}'"))
}

/// `git diff` of `pathspec` from the stage base ([`stage_base`]) to the
/// worktree, with no color, external diff driver or textconv, and binary
/// content shown as text: evidence an adjudicator reads.
pub fn diff_from_base(repo: &WorktreeGit, work_dir: &Path, pathspec: &str) -> Result<String> {
    let base = stage_base(repo, work_dir).context("stage base failed")?;
    let diff = read_only(
        repo,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--text",
            &base,
            "--",
            pathspec,
        ],
    )?;
    Ok(String::from_utf8_lossy(&diff).into_owned())
}

/// Every path changed since `base`: committed on the branch, changed in the
/// working tree or index, or untracked and not ignored. Worktree scaffolding
/// and untracked sandbox artifacts ([`is_tool_artifact`]) are left out. A
/// changed name that is not UTF-8 fails the call, scaffolding or not; one
/// holding a control character fails it (`listed_name`) unless it lies under
/// worktree scaffolding, which is dropped first.
pub fn changed_paths(repo: &WorktreeGit, working_dir: &Path, base: &str) -> Result<Vec<String>> {
    let worktree_root = repo.work_tree();
    let prefix = working_dir_prefix(worktree_root, working_dir)?;
    let committed = DiffNames {
        from: base,
        to: Some("HEAD"),
        filter: None,
        all_submodules: false,
        pathspecs: &[],
    };
    let mut paths = BTreeSet::new();
    for path in changed_names(repo, &committed)? {
        if !is_worktree_scaffold_path(&path) {
            paths.insert(listed_name(path.as_bytes())?);
        }
    }
    let mut status_args = PORCELAIN_V1_Z.to_vec();
    status_args.push("--no-renames");
    paths.extend(status_paths(
        worktree_root,
        &read_only(repo, &status_args)?,
    )?);
    Ok(relative_to(&prefix, paths))
}

/// The repository-relative paths of a porcelain v1 `-z` status listing,
/// without worktree scaffolding and untracked sandbox artifacts.
fn status_paths(worktree_root: &Path, status: &[u8]) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for entry in parse_porcelain_v1_z(status)? {
        if is_worktree_scaffold_path(&entry.path)
            || (entry.is_untracked() && is_tool_artifact(worktree_root, &entry.path))
        {
            continue;
        }
        paths.push(listed_name(entry.path.as_bytes())?);
    }
    Ok(paths)
}

/// Every FIFO, socket and device node in the worktree, relative to the
/// working directory. Git lists none of them, so [`changed_paths`] cannot
/// show one a session planted, yet the next session's `open()` of a FIFO at
/// a path it means to write blocks forever.
///
/// The walk (`special_paths`) opens every directory with a symlink refused
/// at each component and lists it from that descriptor: a symlink is never
/// listed through, and one swapped in for a directory mid-walk fails the call
/// instead of being skipped. It never enters a `.git` directory and skips
/// what git ignores (`ignored_paths`), so `target/` or `node_modules/` are
/// not read. Scaffolding is not skipped: a FIFO there blocks a session as
/// surely as one anywhere else. The walk is no snapshot: an entry made in a
/// directory after it was listed is not seen. A name that is not UTF-8 is
/// shown with `\xNN` escapes (`escaped`).
pub fn special_files(repo: &WorktreeGit, working_dir: &Path) -> Result<Vec<String>> {
    let worktree_root = repo.work_tree();
    let prefix: Vec<String> = working_dir_prefix(worktree_root, working_dir)?
        .iter()
        .map(|part| escaped(part.as_bytes()))
        .collect();
    let ignored = ignored_paths(repo)?;
    Ok(special_paths(worktree_root, &ignored)?
        .iter()
        .map(|path| from_working_dir(&prefix, &escaped(path.as_os_str().as_bytes())))
        .collect())
}

/// The worktree-relative paths git ignores, a wholly ignored directory as
/// one entry.
fn ignored_paths(repo: &WorktreeGit) -> Result<BTreeSet<PathBuf>> {
    let listed = read_only(
        repo,
        &[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
    )?;
    Ok(listed
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| PathBuf::from(OsStr::from_bytes(entry)))
        .collect())
}

/// Tracked and untracked (not ignored) files under the working directory that
/// match a `harness` glob. Only the working directory is listed, so a glob
/// such as `**/*.rs` never reaches a file beside or above it. A match whose
/// name cannot be shown as text fails the call (`listed_name`); a file the
/// globs do not match is no harness file, whatever its name.
pub fn harness_files(
    repo: &WorktreeGit,
    working_dir: &Path,
    harness: &[String],
) -> Result<Vec<String>> {
    if harness.is_empty() {
        return Ok(Vec::new());
    }
    let prefix = working_dir_prefix(repo.work_tree(), working_dir)?;
    let scope = format!(":(literal){}/", prefix.join("/"));
    let mut args = vec![
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ];
    if !prefix.is_empty() {
        args.extend(["--", scope.as_str()]);
    }
    let listed = read_only(repo, &args)?;
    let mut matches = BTreeSet::new();
    for entry in nul_separated(&listed) {
        let shown = String::from_utf8_lossy(entry);
        let path = from_working_dir(&prefix, &shown);
        if is_worktree_scaffold_path(&shown)
            || store::validate_relative(&path).is_err()
            || !harness_matches(&path, harness)
        {
            continue;
        }
        listed_name(entry)?;
        matches.insert(path);
    }
    Ok(matches.into_iter().collect())
}

fn nul_separated(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
}

/// A name git listed, as text. A name that is not UTF-8 or holds a control
/// character is refused, and the refusal shows it escaped ([`escaped`]):
/// lossy or escaped text would name another file to everything that reads
/// or hashes the path, and the raw name would break a refusal message into
/// lines that read as loom's own problems.
fn listed_name(entry: &[u8]) -> Result<String> {
    match std::str::from_utf8(entry) {
        Ok(name) if !name.chars().any(char::is_control) => Ok(name.to_string()),
        _ => bail!(
            "the worktree holds '{}', a file name loom cannot show as text (not UTF-8, or a \
             control character such as a newline in it): rename or remove the file",
            escaped(entry)
        ),
    }
}

/// The working directory's components beneath the worktree root.
fn working_dir_prefix(worktree_root: &Path, working_dir: &Path) -> Result<Vec<String>> {
    let root = worktree_root.canonicalize()?;
    let dir = working_dir.canonicalize()?;
    let inside = dir.strip_prefix(&root).with_context(|| {
        format!(
            "working directory {} is outside the worktree {}",
            dir.display(),
            root.display()
        )
    })?;
    Ok(inside
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect())
}

/// Re-express repository-relative paths relative to the working directory,
/// dropping worktree scaffolding.
fn relative_to(prefix: &[String], paths: BTreeSet<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| !is_worktree_scaffold_path(path))
        .map(|path| from_working_dir(prefix, &path))
        .collect()
}

/// A repository-relative path re-expressed relative to the working directory.
fn from_working_dir(prefix: &[String], path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > prefix.len() && parts.iter().zip(prefix).all(|(a, b)| a == b) {
        parts[prefix.len()..].join("/")
    } else {
        format!("{}{path}", "../".repeat(prefix.len()))
    }
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
