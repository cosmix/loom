use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::placement::{place_skill, remove_empty_catalog, skill_names};
use crate::assets::Asset;
use crate::skills::{is_core_skill, SkillLayout, CATALOG_DIR_NAME};

pub(super) fn install_files(
    claude_dir: &Path,
    agents: &[Asset],
    commands: &[Asset],
) -> Result<(usize, usize, Vec<PathBuf>)> {
    let (agents, mut backups) = write_assets(&claude_dir.join("agents"), agents)?;
    let (commands, mut command_backups) = write_assets(&claude_dir.join("commands"), commands)?;
    backups.append(&mut command_backups);
    Ok((agents, commands, backups))
}

pub(super) fn install_skills(
    claude_dir: &Path,
    assets: &[Asset],
    layout: SkillLayout,
) -> Result<(usize, usize)> {
    let skills_dir = claude_dir.join("skills");
    let catalog_dir = claude_dir.join(CATALOG_DIR_NAME);
    let mut resident = 0;
    let mut catalogued = 0;
    for name in skill_names(assets) {
        let in_skills = layout == SkillLayout::All || is_core_skill(name);
        place_skill(name, assets, &skills_dir, &catalog_dir, in_skills)?;
        if in_skills {
            resident += 1;
        } else {
            catalogued += 1;
        }
    }
    if layout == SkillLayout::All {
        remove_empty_catalog(&catalog_dir)?;
    }
    Ok((resident, catalogued))
}

pub(super) fn write_layout(claude_dir: &Path, layout: SkillLayout) -> Result<()> {
    fs::create_dir_all(claude_dir)
        .with_context(|| format!("Failed to create {}", claude_dir.display()))?;
    let value = match layout {
        SkillLayout::Core => "core",
        SkillLayout::All => "all",
    };
    let path = claude_dir.join("loom-install.toml");
    fs::write(&path, format!("# Managed by loom\nskills = \"{value}\"\n"))
        .with_context(|| format!("Failed to write {}", path.display()))
}

/// Write every embedded agent or command asset to `root`, preserving anything
/// loom did not put there.
///
/// Unrecognized differing content is copied to a unique backup on every
/// replacement; prior backups remain recoverable. Identical content is left
/// untouched. An upgrade can therefore also preserve a prior shipped version.
///
/// A destination that is a live symlink is written through rather than
/// replaced: `~/.claude/commands/` (or `agents/`) is sometimes itself a
/// symlink into a dotfiles repository, the same case `write_managed_file`
/// deliberately supports for the doctrine files, so the backed-up copy is
/// taken from the link's target and the new content lands back through the
/// link, leaving the link itself in place. A dangling symlink - one whose
/// target no longer exists - is refused instead: writing through it would
/// create a file at the target, outside the tree loom was asked to install.
fn write_assets(root: &Path, assets: &[Asset]) -> Result<(usize, Vec<PathBuf>)> {
    let mut backups = Vec::new();
    for &(path, content) in assets {
        let dest = root.join(path);
        let parent = dest
            .parent()
            .context("Embedded asset destination has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
        super::ensure_not_dangling_symlink(&dest)?;
        let existing = match fs::read(&dest) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| format!("Failed to read {}", dest.display()))
            }
        };
        match existing {
            Some(bytes) if bytes == content.as_bytes() => continue,
            Some(_) => {
                backups.push(super::back_up_asset(&dest)?);
            }
            None => {}
        }
        fs::write(&dest, content).with_context(|| format!("Failed to write {}", dest.display()))?;
    }
    Ok((assets.len(), backups))
}
