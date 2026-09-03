use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::placement::{place_skill, remove_empty_catalog, skill_names};
use crate::assets::Asset;
use crate::skills::{is_core_skill, SkillLayout, CATALOG_DIR_NAME};

pub(super) fn install_files(
    claude_dir: &Path,
    agents: &[Asset],
    commands: &[Asset],
) -> Result<(usize, usize)> {
    let agents = write_assets(&claude_dir.join("agents"), agents)?;
    let commands = write_assets(&claude_dir.join("commands"), commands)?;
    Ok((agents, commands))
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

fn write_assets(root: &Path, assets: &[Asset]) -> Result<usize> {
    for &(path, content) in assets {
        let dest = root.join(path);
        let parent = dest
            .parent()
            .context("Embedded asset destination has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
        fs::write(&dest, content).with_context(|| format!("Failed to write {}", dest.display()))?;
    }
    Ok(assets.len())
}
