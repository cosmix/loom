//! A skill's `references/` directory ships with the skill: the build embeds
//! every file under a `loom-*` skill directory, and install keeps the nested
//! path, so a `SKILL.md` link such as `references/sandbox.md` resolves after
//! install.

use super::{install, paths};
use crate::assets::SKILLS;
use crate::skills::SkillLayout;
use tempfile::TempDir;

const REFERENCES_PREFIX: &str = "loom-plan-writer/references/";

#[test]
fn plan_writer_references_are_embedded() {
    let references: Vec<&str> = SKILLS
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| key.starts_with(REFERENCES_PREFIX))
        .collect();

    assert!(
        !references.is_empty(),
        "SKILLS must embed loom-plan-writer/references/*.md"
    );
    assert!(
        references.contains(&"loom-plan-writer/references/sandbox.md"),
        "SKILLS is missing loom-plan-writer/references/sandbox.md: {references:?}"
    );
}

#[test]
fn installed_plan_writer_keeps_the_references_directory() {
    let temp = TempDir::new().unwrap();
    install(&temp, SkillLayout::Core);

    let reference = paths(&temp)
        .claude_dir
        .join("skills/loom-plan-writer/references/sandbox.md");
    assert!(
        reference.is_file(),
        "{} must be installed beside SKILL.md",
        reference.display()
    );
}
