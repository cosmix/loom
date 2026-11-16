//! Check 3 of `admit_start`: the brief file (common.md section 13).
//!
//! Containment is decided on the opened file, never on the path string: the
//! path is only split into the canonical worktree and a relative remainder,
//! and the remainder is opened from a descriptor of the worktree with every
//! component `O_NOFOLLOW`. A symlinked leaf or directory, a hard link, a FIFO
//! or a `..` component therefore cannot reach a file outside the worktree.

use std::path::{Component, Path, PathBuf};

use super::StartRequest;
use crate::fs::safe_read::{open_regular_no_follow, read_open_bounded};
use crate::fs::work_dir::WorkDir;
use crate::git::worktree::stage_worktree_path;
use crate::models::stage::Stage;
use crate::native_worker::model::MAX_BRIEF_BYTES;
use crate::utils::sha256_hex;

/// Reads the brief `request` names. `Ok` is the canonical stage worktree (the
/// manifest's `workspace_root`) and the exact bytes read, whose sha256 equals
/// `request.brief_sha256`.
pub(super) fn read_brief(
    work_dir: &Path,
    stage: &Stage,
    request: &StartRequest,
) -> Result<(PathBuf, Vec<u8>), String> {
    if !request.brief_path.is_absolute() {
        return Err(format!(
            "the brief path '{}' is not absolute",
            request.brief_path.display()
        ));
    }
    let root = canonical_worktree(work_dir, stage)?;
    let relative = contained_relative(&request.brief_path, &root, stage, work_dir)?;
    let bytes = read_contained(&root, &relative)?;
    if std::str::from_utf8(&bytes).is_err() {
        return Err("the brief is not valid UTF-8".to_string());
    }
    if sha256_hex(&bytes) != request.brief_sha256 {
        return Err("brief_sha256 does not match the brief file".to_string());
    }
    Ok((root, bytes))
}

fn repo_root(work_dir: &Path) -> Result<PathBuf, String> {
    let unresolved = || format!("no repository root above {}", work_dir.display());
    WorkDir::new(work_dir)
        .map_err(|_| unresolved())?
        .main_project_root()
        .ok_or_else(unresolved)
}

fn canonical_worktree(work_dir: &Path, stage: &Stage) -> Result<PathBuf, String> {
    let worktree =
        stage_worktree_path(stage, &repo_root(work_dir)?).map_err(|error| format!("{error:#}"))?;
    worktree
        .canonicalize()
        .map_err(|_| "stage worktree is missing".to_string())
}

/// The part of `brief_path` below the worktree, as plain components only.
fn contained_relative(
    brief_path: &Path,
    root: &Path,
    stage: &Stage,
    work_dir: &Path,
) -> Result<String, String> {
    let outside = || {
        format!(
            "the brief must be inside the stage worktree {}",
            root.display()
        )
    };
    let relative = match brief_path.strip_prefix(root) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => {
            // The lead may name the worktree through its uncanonical path.
            let worktree = stage_worktree_path(stage, &repo_root(work_dir)?)
                .map_err(|error| format!("{error:#}"))?;
            brief_path
                .strip_prefix(&worktree)
                .map_err(|_| outside())?
                .to_path_buf()
        }
    };
    let plain = relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if !plain || relative.as_os_str().is_empty() {
        return Err(outside());
    }
    relative.to_str().map(str::to_string).ok_or_else(outside)
}

fn read_contained(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let file = open_regular_no_follow(root, relative, libc::O_RDONLY)
        .map_err(|error| format!("the brief cannot be opened safely: {error:#}"))?
        .ok_or_else(|| "the brief file does not exist".to_string())?;
    read_open_bounded(file, Path::new(relative), MAX_BRIEF_BYTES as usize)
        .map_err(|error| format!("the brief cannot be read: {error:#}"))
}
