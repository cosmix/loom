use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;

use super::placement::{place_skill, remove_empty_catalog, skill_names};
use crate::assets::Asset;
use crate::skills::{is_core_skill, SkillLayout, CATALOG_DIR_NAME};

/// Every skill directory [`install_skills`] may place in `<codex>/skills`,
/// under either layout: each shared skill and each Codex-only one.
pub(crate) fn codex_skill_dir_names() -> BTreeSet<&'static str> {
    let mut names = skill_names(crate::assets::SKILLS);
    names.extend(skill_names(crate::assets::CODEX_SKILLS));
    names
}

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
