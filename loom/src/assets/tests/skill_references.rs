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
    for name in ["sandbox.md", "plan-template.md"] {
        let key = format!("{REFERENCES_PREFIX}{name}");
        assert!(
            references.contains(&key.as_str()),
            "SKILLS is missing {key}: {references:?}"
        );
    }
}

#[test]
fn installed_plan_writer_keeps_the_references_directory() {
    let temp = TempDir::new().unwrap();
    install(&temp, SkillLayout::Core);

    let paths = paths(&temp);
    for root in [paths.claude_dir, paths.codex_dir] {
        for name in ["sandbox.md", "plan-template.md"] {
            let key = format!("{REFERENCES_PREFIX}{name}");
            let reference = root.join("skills").join(&key);
            let (_, expected) = SKILLS.iter().find(|(path, _)| *path == key).unwrap();
            assert_eq!(
                std::fs::read_to_string(&reference).unwrap(),
                *expected,
                "{} must be installed intact beside SKILL.md",
                reference.display()
            );
        }
    }
}
