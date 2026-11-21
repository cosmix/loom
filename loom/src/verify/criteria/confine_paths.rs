//! Path resolution for a confined command: its program and its working tree.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn resolve_program(original: &Command, cwd: &Path) -> Result<PathBuf> {
    let program = original.get_program();
    let path = original
        .get_envs()
        .find_map(|(k, v)| (k == "PATH").then_some(v).flatten());
    which::which_in(program, path, cwd)
        .with_context(|| format!("Failed to resolve confined command: {:?}", program))
}

/// The tree a command starting in the canonical `cwd` may use: `tree`, which
/// must contain `cwd`, else `cwd` itself. The tree is writable to the code it
/// holds, which could swap it for a symlink after a full resolve; only its
/// parent is resolved, and the tree itself must be a directory, so the
/// boundary grants that path and never a link's target.
pub(super) fn confined_tree(tree: Option<&Path>, cwd: &Path) -> Result<PathBuf> {
    let Some(tree) = tree else {
        return Ok(cwd.to_path_buf());
    };
    let tree = match (tree.parent(), tree.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            parent.canonicalize().map(|parent| parent.join(name))
        }
        _ => tree.canonicalize(),
    }
    .context("resolve confined working tree")?;
    let metadata = tree
        .symlink_metadata()
        .context("resolve confined working tree")?;
    anyhow::ensure!(
        metadata.is_dir(),
        "confined working tree {} must be a directory, not a symlink",
        tree.display()
    );
    anyhow::ensure!(
        cwd.starts_with(&tree),
        "confined working directory {} is outside its working tree {}",
        cwd.display(),
        tree.display()
    );
    Ok(tree)
}
