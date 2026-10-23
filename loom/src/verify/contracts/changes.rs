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
use crate::git::worktree::{is_worktree_scaffold_path, WorktreeGit};
use crate::verify::tool_artifacts::is_tool_artifact;

/// Global options for every git call here: no index refresh written back,
/// and no fsmonitor command taken from a config the stage can edit.
const READ_ONLY: [&str; 3] = ["--no-optional-locks", "-c", "core.fsmonitor=false"];

/// The commit the stage branch forked from: the merge base of `HEAD` and the
/// merge target configured in the state directory `work_dir`, a default
/// target resolved in the repository that holds `work_dir`.
pub fn stage_base(repo: &WorktreeGit, work_dir: &Path) -> Result<String> {
    let workspace = WorkDir::new(work_dir)?;
    let project = workspace
        .repo_root()
        .context("cannot resolve the repository root of the state directory")?;
    let target = crate::fs::resolve_target_branch_from_config(work_dir, project)?;
    let output = git(repo, &["merge-base", &target, "HEAD"])?;
    let base = String::from_utf8_lossy(&output).trim().to_string();
    if base.is_empty() {
        bail!("no merge base between HEAD and '{target}'");
    }
    Ok(base)
}

/// `git diff` of `pathspec` from the stage base ([`stage_base`]) to the
/// worktree, with no color, external diff driver or textconv, and binary
/// content shown as text: evidence an adjudicator reads.
pub fn diff_from_base(repo: &WorktreeGit, work_dir: &Path, pathspec: &str) -> Result<String> {
    let base = stage_base(repo, work_dir).context("stage base failed")?;
    let diff = git(
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
/// changed name that cannot be shown as text fails the call (`listed_name`),
/// unless it lies under worktree scaffolding, which is dropped first.
pub fn changed_paths(repo: &WorktreeGit, working_dir: &Path, base: &str) -> Result<Vec<String>> {
    let worktree_root = repo.work_tree();
    let prefix = working_dir_prefix(worktree_root, working_dir)?;
    let committed = changed_names(repo, base, Some("HEAD"))?;
    let status = git(
        repo,
        &["status", "--porcelain", "-z", "-uall", "--no-renames"],
    )?;
    let mut paths = nul_separated(&committed)
        .filter_map(listed_unless_scaffold)
        .collect::<Result<BTreeSet<String>>>()?;
    for path in status_paths(worktree_root, &status) {
        paths.insert(path?);
    }
    Ok(relative_to(&prefix, paths))
}

/// The NUL-separated paths `git diff --name-only` lists between the commit
/// `from` and the commit `to`, or the working tree when `to` is `None`, with
/// neither rename detection nor an external diff driver.
pub(in crate::verify) fn changed_names(
    repo: &WorktreeGit,
    from: &str,
    to: Option<&str>,
) -> Result<Vec<u8>> {
    let mut args = vec![
        "diff",
        "--name-only",
        "-z",
        "--no-renames",
        "--no-ext-diff",
        from,
    ];
    args.extend(to);
    args.push("--");
    git(repo, &args)
}

/// The repository-relative paths of porcelain v1 `-z` status entries
/// (`XY <path>`), without the untracked sandbox artifacts.
fn status_paths<'a>(
    worktree_root: &'a Path,
    status: &'a [u8],
) -> impl Iterator<Item = Result<String>> + 'a {
    nul_separated(status).filter_map(move |entry| {
        let path = entry.get(3..).and_then(listed_unless_scaffold)?;
        let placeholder = entry.starts_with(b"??")
            && path
                .as_ref()
                .is_ok_and(|path| is_tool_artifact(worktree_root, path));
        (!placeholder).then_some(path)
    })
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
    let listed = git(
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
    let listed = git(repo, &args)?;
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

/// `git <args>` through `repo`, read-only (`READ_ONLY`), its stdout on
/// success.
pub(in crate::verify) fn git(repo: &WorktreeGit, args: &[&str]) -> Result<Vec<u8>> {
    let argv: Vec<&str> = READ_ONLY.iter().chain(args).copied().collect();
    let output = repo.run(&argv)?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo.work_tree().display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
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

/// [`listed_name`] of `entry`, or `None` for a worktree scaffold path: loom's
/// own scaffolding is never a change, so an odd name under it cannot refuse a
/// freeze. The scaffold test reads the lossy text, which keeps every scaffold
/// prefix intact.
fn listed_unless_scaffold(entry: &[u8]) -> Option<Result<String>> {
    if is_worktree_scaffold_path(&String::from_utf8_lossy(entry)) {
        return None;
    }
    Some(listed_name(entry))
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
