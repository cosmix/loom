use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::assets::Asset;
use crate::skills::{is_core_skill, SkillLayout, CATALOG_DIR_NAME};

pub(super) fn install_skills(
    codex_dir: &Path,
    skills: &[Asset],
    codex_skills: &[Asset],
    layout: SkillLayout,
) -> Result<(usize, usize)> {
    let skills_dir = codex_dir.join("skills");
    let catalog_dir = codex_dir.join(CATALOG_DIR_NAME);
    let names = skill_names(skills);
    let codex_names = skill_names(codex_skills);
    let mut resident = 0;
    let mut catalogued = 0;
    for &name in &names {
        let from_codex = codex_names.contains(name);
        let in_skills = from_codex || layout == SkillLayout::All || is_core_skill(name);
        let source = if from_codex { codex_skills } else { skills };
        place_skill(name, source, &skills_dir, &catalog_dir, in_skills)?;
        if in_skills {
            resident += 1;
        } else {
            catalogued += 1;
        }
    }
    for &name in codex_names.difference(&names) {
        place_skill(name, codex_skills, &skills_dir, &catalog_dir, true)?;
        resident += 1;
    }
    if layout == SkillLayout::All {
        remove_empty_catalog(&catalog_dir)?;
    }
    Ok((resident, catalogued))
}

fn skill_names(assets: &[Asset]) -> BTreeSet<&'static str> {
    assets
        .iter()
        .filter_map(|(path, _)| path.split_once('/').map(|(name, _)| name))
        .collect()
}

fn place_skill(
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
    if other.exists() {
        fs::remove_dir_all(&other)
            .with_context(|| format!("Failed to remove {}", other.display()))?;
    }
    write_skill(&dest, assets, name)
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
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
            fs::write(&dest, content)
                .with_context(|| format!("Failed to write {}", dest.display()))?;
        }
    }
    Ok(())
}

fn remove_empty_catalog(catalog_dir: &Path) -> Result<()> {
    if catalog_dir.is_dir()
        && fs::read_dir(catalog_dir)
            .with_context(|| format!("Failed to read {}", catalog_dir.display()))?
            .next()
            .is_none()
    {
        fs::remove_dir(catalog_dir)
            .with_context(|| format!("Failed to remove {}", catalog_dir.display()))?;
    }
    Ok(())
}
