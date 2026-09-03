//! Placing one embedded skill directory, shared by the Claude and Codex
//! installers: both put the same skill trees under a resident `skills/` root
//! or a catalog root, and differ only in which assets they draw from.

use anyhow::{ensure, Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use crate::assets::Asset;

/// The distinct skill names in an asset table (the first path component).
pub(super) fn skill_names(assets: &[Asset]) -> BTreeSet<&'static str> {
    assets
        .iter()
        .filter_map(|(path, _)| path.split_once('/').map(|(name, _)| name))
        .collect()
}

/// Write `name` into its chosen root and clear it out of the other one.
///
/// The destination is written before the other location is removed, so a
/// failed write never leaves the skill missing from both roots.
///
/// A symlinked destination is unlinked and replaced by a real directory, the
/// opposite of the doctrine files, which are deliberately written through their
/// link. The two differ because a skill directory is pruned: following the link
/// would let `prune_dir` delete files outside the tree loom was asked to write.
pub(in crate::assets) fn place_skill(
    name: &str,
    assets: &[Asset],
    skills_dir: &Path,
    catalog_dir: &Path,
    in_skills: bool,
) -> Result<()> {
    let (dest, other) = if in_skills {
        (skills_dir.join(name), catalog_dir.join(name))
    } else {
        (catalog_dir.join(name), skills_dir.join(name))
    };
    if let Some(metadata) = symlink_metadata(&dest)? {
        if !metadata.is_dir() {
            remove_file(&dest)?;
        }
    }
    write_skill(&dest, assets, name)?;
    prune_skill(&dest, assets, name)?;
    remove_any(&other)
}

fn write_skill(root: &Path, assets: &[Asset], name: &str) -> Result<()> {
    for &(path, content) in assets {
        let Some((asset_name, relative)) = path.split_once('/') else {
            continue;
        };
        if asset_name == name {
            let dest = root.join(relative);
            let parent = dest
                .parent()
                .context("Embedded skill destination has no parent directory")?;
            clear_non_directory_ancestors(root, parent)?;
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
            fs::write(&dest, content)
                .with_context(|| format!("Failed to write {}", dest.display()))?;
        }
    }
    Ok(())
}

/// Unlink any non-directory sitting where a subdirectory belongs, between
/// `root` and `parent`, before `create_dir_all` walks the same path.
///
/// `create_dir_all` follows an existing symlinked directory rather than
/// replacing it, so a subdirectory that happens to be a symlink would send
/// every asset under it to wherever the link points, outside the tree loom
/// was asked to write. `place_skill` already applies this unlink-and-replace
/// policy to the skill root itself; this extends it to every component below
/// it, which today's asset table never nests deep enough to exercise, but a
/// future skill with a subdirectory would otherwise hit silently.
fn clear_non_directory_ancestors(root: &Path, parent: &Path) -> Result<()> {
    let relative = parent
        .strip_prefix(root)
        .context("Skill destination parent is not under its root")?;
    // `strip_prefix` only rejects an absolute asset key; a relative one
    // containing a `..` component still lexically "starts with" `root`
    // (`root/..` is itself a real directory), so it passes `strip_prefix` and
    // would send `create_dir_all`/`fs::write` below outside `root` with
    // nothing here to stop them. Requiring every remaining component to be
    // `Normal` closes that gap at runtime, alongside the compile-time
    // `assert_clean_keys` check on the embedded tables themselves.
    ensure!(
        relative
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "Skill destination {} escapes its root {} via a non-normal path component",
        parent.display(),
        root.display()
    );
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if let Some(metadata) = symlink_metadata(&current)? {
            if !metadata.is_dir() {
                remove_any(&current)?;
            }
        }
    }
    Ok(())
}

/// Drop anything under `root` the current asset table no longer ships, so a
/// file removed from a skill upstream does not linger forever.
///
/// Only directories named by the asset table are ever pruned, which keeps an
/// operator's own skill directories out of reach.
fn prune_skill(root: &Path, assets: &[Asset], name: &str) -> Result<()> {
    let keep: BTreeSet<&Path> = assets
        .iter()
        .filter_map(|(path, _)| path.split_once('/'))
        .filter(|(asset_name, _)| *asset_name == name)
        .map(|(_, relative)| Path::new(relative))
        .collect();
    // Without this, a `name` absent from `assets` would prune everything under
    // `root` instead of failing: an emptied directory, not an error.
    ensure!(
        !keep.is_empty(),
        "The asset table ships no files for the skill {name}"
    );
    prune_dir(root, root, &keep)
}

fn prune_dir(dir: &Path, root: &Path, keep: &BTreeSet<&Path>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("Failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("Failed to inspect {}", dir.display()))?;
        let path = entry.path();
        let is_dir = entry
            .file_type()
            .with_context(|| format!("Failed to inspect {}", path.display()))?
            .is_dir();
        if is_dir {
            prune_dir(&path, root, keep)?;
            remove_if_empty(&path)?;
        } else if !keep.contains(path.strip_prefix(root)?) {
            remove_file(&path)?;
        }
    }
    Ok(())
}

fn remove_if_empty(dir: &Path) -> Result<()> {
    let empty = fs::read_dir(dir)
        .with_context(|| format!("Failed to read {}", dir.display()))?
        .next()
        .is_none();
    if empty {
        fs::remove_dir(dir).with_context(|| format!("Failed to remove {}", dir.display()))?;
    }
    Ok(())
}

/// Remove `path` whatever it is: a plain file or a symlink where a skill
/// directory is expected would make `remove_dir_all` fail with `ENOTDIR` and
/// abort the install half-written.
fn remove_any(path: &Path) -> Result<()> {
    let Some(metadata) = symlink_metadata(path)? else {
        return Ok(());
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("Failed to remove {}", path.display()))
    } else {
        remove_file(path)
    }
}

/// `path`'s own metadata, following no link, and `None` when nothing is there.
pub(super) fn symlink_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match path.symlink_metadata() {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to inspect {}", path.display())),
    }
}

fn remove_file(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("Failed to remove {}", path.display()))
}

/// Drop the catalog directory once the `All` layout has emptied it.
///
/// A symlinked catalog is left alone: `is_dir` would follow the link and send
/// `remove_dir` at the link path, which fails with `ENOTDIR`.
pub(super) fn remove_empty_catalog(catalog_dir: &Path) -> Result<()> {
    if symlink_metadata(catalog_dir)?.is_some_and(|metadata| metadata.is_dir()) {
        remove_if_empty(catalog_dir)?;
    }
    Ok(())
}
