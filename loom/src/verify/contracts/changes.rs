//! What a contract session changed and which files the freeze covers, read
//! from the stage worktree with read-only git (DESIGN D8 step 1).
//!
//! Paths come back relative to the stage's working directory, the frame
//! contract `file`s and `harness` globs are written in. A path outside it
//! keeps its `../` prefix, so it can never pass as a contract or harness file.

use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::{Component, Path};

use super::{harness_matches, store};
use crate::git::runner::run_git;
use crate::git::worktree::is_worktree_scaffold_path;

/// Global options for every git call here: no index refresh written back,
/// and no fsmonitor command taken from a config the stage can edit.
const READ_ONLY: [&str; 3] = ["--no-optional-locks", "-c", "core.fsmonitor=false"];

/// The commit the stage branch forked from: the merge base of `HEAD` and the
/// configured merge target.
pub fn stage_base(worktree_root: &Path, work_dir: &Path) -> Result<String> {
    let target = crate::fs::resolve_target_branch_from_config(work_dir, worktree_root)?;
    let output = git(worktree_root, &["merge-base", &target, "HEAD"])?;
    let base = String::from_utf8_lossy(&output).trim().to_string();
    if base.is_empty() {
        bail!("no merge base between HEAD and '{target}'");
    }
    Ok(base)
}

/// Every path changed since `base`: committed on the branch, changed in the
/// working tree or index, or untracked and not ignored. Worktree scaffolding
/// is left out.
pub fn changed_paths(worktree_root: &Path, working_dir: &Path, base: &str) -> Result<Vec<String>> {
    let prefix = working_dir_prefix(worktree_root, working_dir)?;
    let committed = git(
        worktree_root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            base,
            "HEAD",
        ],
    )?;
    let status = git(
        worktree_root,
        &["status", "--porcelain", "-z", "-uall", "--no-renames"],
    )?;
    let mut paths: BTreeSet<String> = nul_separated(&committed).collect();
    // Porcelain v1 with -z: "XY <path>", repository-relative.
    paths.extend(nul_separated(&status).filter_map(|entry| entry.get(3..).map(str::to_string)));
    Ok(relative_to(&prefix, paths))
}

/// Tracked and untracked (not ignored) files under the working directory that
/// match a `harness` glob. Only the working directory is listed, so a glob
/// such as `**/*.rs` never reaches a file beside or above it.
pub fn harness_files(
    worktree_root: &Path,
    working_dir: &Path,
    harness: &[String],
) -> Result<Vec<String>> {
    if harness.is_empty() {
        return Ok(Vec::new());
    }
    let prefix = working_dir_prefix(worktree_root, working_dir)?;
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
    let listed = git(worktree_root, &args)?;
    let paths: BTreeSet<String> = nul_separated(&listed).collect();
    Ok(relative_to(&prefix, paths)
        .into_iter()
        .filter(|path| store::validate_relative(path).is_ok() && harness_matches(path, harness))
        .collect())
}

fn git(worktree_root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let argv: Vec<&str> = READ_ONLY.iter().chain(args).copied().collect();
    let output = run_git(&argv, worktree_root)?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            worktree_root.display(),
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
        .map(|path| {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() > prefix.len() && parts.iter().zip(prefix).all(|(a, b)| a == b) {
                parts[prefix.len()..].join("/")
            } else {
                format!("{}{path}", "../".repeat(prefix.len()))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let files = harness_files(root, &root.join("loom"), &["**/*.rs".to_string()]).unwrap();

        assert_eq!(files, vec!["src/inside.rs"]);
    }
}
