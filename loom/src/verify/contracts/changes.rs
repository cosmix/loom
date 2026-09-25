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
/// and untracked sandbox artifacts ([`is_tool_artifact`]) are left out.
pub fn changed_paths(repo: &WorktreeGit, working_dir: &Path, base: &str) -> Result<Vec<String>> {
    let worktree_root = repo.work_tree();
    let prefix = working_dir_prefix(worktree_root, working_dir)?;
    let committed = changed_names(repo, base, Some("HEAD"))?;
    let status = git(
        repo,
        &["status", "--porcelain", "-z", "-uall", "--no-renames"],
    )?;
    let mut paths: BTreeSet<String> = nul_separated(&committed).collect();
    paths.extend(status_paths(worktree_root, &status));
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
) -> impl Iterator<Item = String> + 'a {
    nul_separated(status).filter_map(move |entry| {
        let path = entry.get(3..)?;
        let placeholder = entry.starts_with("??") && is_tool_artifact(worktree_root, path);
        (!placeholder).then(|| path.to_string())
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
/// such as `**/*.rs` never reaches a file beside or above it.
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
    let paths: BTreeSet<String> = nul_separated(&listed).collect();
    Ok(relative_to(&prefix, paths)
        .into_iter()
        .filter(|path| store::validate_relative(path).is_ok() && harness_matches(path, harness))
        .collect())
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

fn nul_separated(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| String::from_utf8_lossy(entry).into_owned())
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
mod tests {
    use super::*;
    use crate::git::runner::run_git;
    use crate::verify::contracts::test_support::{contract_worktree, pinned};

    fn set(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    #[test]
    fn paths_are_expressed_relative_to_the_working_directory() {
        let prefix = vec!["loom".to_string()];
        let paths = set(&[
            "loom/tests/a.rs",
            "doc/x.md",
            "loom",
            ".claude/settings.json",
        ]);
        assert_eq!(
            relative_to(&prefix, paths),
            vec!["../doc/x.md", "../loom", "tests/a.rs"]
        );
    }

    #[test]
    fn at_the_worktree_root_paths_are_unchanged() {
        assert_eq!(
            relative_to(&[], set(&["src/lib.rs", ".loom/work/x"])),
            vec!["src/lib.rs"]
        );
    }

    #[test]
    fn harness_files_stay_inside_the_working_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        assert!(run_git(&["init", "-q"], root).unwrap().status.success());
        for file in ["loom/src/inside.rs", "other/outside.rs"] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }

        let repo = WorktreeGit::discovered(root);
        let files = harness_files(&repo, &root.join("loom"), &["**/*.rs".to_string()]).unwrap();

        assert_eq!(files, vec!["src/inside.rs"]);
    }

    /// The sandbox's placeholders at worktree-root dotfiles are not changes
    /// the contract phase made; the same name tracked and modified, content
    /// at a listed name, and a FIFO a session made are.
    #[test]
    fn untracked_sandbox_artifacts_are_not_changes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".bashrc"), b"").unwrap();
        std::fs::write(root.join(".gitconfig"), b"").unwrap();
        std::fs::write(root.join(".mcp.json"), b"{}").unwrap();
        nix::unistd::mkfifo(&root.join("pipe"), nix::sys::stat::Mode::S_IRWXU).unwrap();
        let status = b"?? .bashrc\0?? .mcp.json\0?? pipe\0 M .gitconfig\0";

        let listed: Vec<String> = status_paths(root, status).collect();

        assert_eq!(listed, vec![".mcp.json", "pipe", ".gitconfig"]);
    }

    /// Git lists no FIFO, so the walk finds it; one inside an ignored
    /// directory or `.git` is not looked for, and a symlink is not followed.
    #[test]
    fn special_files_are_found_outside_ignored_directories() {
        let tmp = tempfile::TempDir::new().unwrap();
        let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
        for dir in ["loom/src", "target", "other/.git"] {
            std::fs::create_dir_all(worktree.join(dir)).unwrap();
        }
        std::fs::write(worktree.join(".gitignore"), "target/\n").unwrap();
        for fifo in [
            "loom/src/lib.rs",
            "other/pipe",
            "other/.git/pipe",
            "target/pipe",
        ] {
            nix::unistd::mkfifo(&worktree.join(fifo), nix::sys::stat::Mode::S_IRWXU).unwrap();
        }
        std::os::unix::fs::symlink("/dev", worktree.join("devices")).unwrap();

        let found = special_files(&pinned(&worktree), &worktree.join("loom")).unwrap();

        assert_eq!(found, vec!["src/lib.rs", "../other/pipe"]);
    }

    /// A symlink to a directory outside the worktree, where a session may
    /// swap one in for a directory, is not listed: the FIFOs behind it are
    /// not named.
    #[test]
    fn special_files_behind_a_symlinked_directory_are_not_named() {
        let tmp = tempfile::TempDir::new().unwrap();
        let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(outside.join("nested")).unwrap();
        for fifo in ["pipe", "nested/pipe"] {
            nix::unistd::mkfifo(&outside.join(fifo), nix::sys::stat::Mode::S_IRWXU).unwrap();
        }
        std::os::unix::fs::symlink(&outside, worktree.join("swapped")).unwrap();

        assert!(special_files(&pinned(&worktree), &worktree)
            .unwrap()
            .is_empty());
    }

    /// Names that are not UTF-8 are reported with byte escapes, not mangled.
    #[cfg(target_os = "linux")]
    #[test]
    fn special_files_with_non_utf8_names_are_named_losslessly() {
        let tmp = tempfile::TempDir::new().unwrap();
        let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
        let dir = worktree.join("loom").join(OsStr::from_bytes(b"d\xfe"));
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join(OsStr::from_bytes(b"p\xff"));
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();

        let found = special_files(&pinned(&worktree), &worktree.join("loom")).unwrap();

        assert_eq!(found, vec!["d\\xfe/p\\xff"]);
    }
}
