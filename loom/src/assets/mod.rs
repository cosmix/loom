//! Every installable asset, embedded at build time.

/// (path relative to the group's root, file contents)
pub type Asset = (&'static str, &'static str);

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

pub mod install;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_table {
    use super::{
        Asset, AGENTS_MD_TEMPLATE, CLAUDE_AGENTS, CLAUDE_COMMANDS, CLAUDE_MD_TEMPLATE,
        CODEX_SKILLS, SKILLS,
    };

    // Codex loads ~/.codex/AGENTS.md into every session and truncates a project
    // doc at its project_doc_max_bytes default of 32,768 bytes, so Loom keeps a tighter ceiling.
    const AGENTS_MD_TEMPLATE_MAX_BYTES: usize = 12_288;

    #[test]
    fn all_asset_groups_are_non_empty() {
        for (name, assets) in groups() {
            assert!(!assets.is_empty(), "{name} must not be empty");
        }
    }

    #[test]
    fn skills_include_required_files() {
        for key in [
            "loom-plan-writer/SKILL.md",
            "loom-rust/SKILL.md",
            "loom-md-tables/fix-md-tables.py",
        ] {
            assert!(has_key(SKILLS, key), "SKILLS is missing {key}");
        }
        assert!(
            SKILLS
                .iter()
                .all(|(key, _)| !key.starts_with("core-skills")),
            "SKILLS must not embed the core-skills manifest"
        );
    }

    #[test]
    fn codex_and_claude_tables_include_required_files() {
        assert!(has_key(CODEX_SKILLS, "pressure/SKILL.md"));
        assert!(has_key(CODEX_SKILLS, "loom-skills/SKILL.md"));
        assert!(has_key(CLAUDE_AGENTS, "loom-software-engineer.md"));
        assert!(has_key(CLAUDE_COMMANDS, "pressure.md"));
    }

    #[test]
    fn asset_keys_are_clean_sorted_and_unique() {
        for (name, assets) in groups() {
            assert_clean_keys(name, assets);
            assert_sorted_and_unique(name, assets);
        }
    }

    #[test]
    fn templates_have_expected_contents_and_size() {
        assert!(
            CLAUDE_MD_TEMPLATE.starts_with("# CLAUDE.md - BINDING RULES"),
            "CLAUDE_MD_TEMPLATE must retain its binding-rules heading"
        );
        assert!(
            !AGENTS_MD_TEMPLATE.is_empty(),
            "AGENTS_MD_TEMPLATE must not be empty"
        );
        let actual = AGENTS_MD_TEMPLATE.len();
        assert!(
            actual <= AGENTS_MD_TEMPLATE_MAX_BYTES,
            "AGENTS.md.template regrew to {actual} bytes (ceiling \
             {AGENTS_MD_TEMPLATE_MAX_BYTES}). Codex loads ~/.codex/AGENTS.md into every session \
             and truncates a project doc at its project_doc_max_bytes default of 32768 bytes. \
             Trim the regrowth instead of raising the ceiling."
        );
    }

    fn groups() -> [(&'static str, &'static [Asset]); 4] {
        [
            ("CLAUDE_AGENTS", CLAUDE_AGENTS),
            ("CLAUDE_COMMANDS", CLAUDE_COMMANDS),
            ("SKILLS", SKILLS),
            ("CODEX_SKILLS", CODEX_SKILLS),
        ]
    }

    fn has_key(assets: &[Asset], expected: &str) -> bool {
        assets.iter().any(|(key, _)| *key == expected)
    }

    fn assert_clean_keys(name: &str, assets: &[Asset]) {
        for (key, _) in assets {
            assert!(
                !key.contains("__pycache__") && !key.contains('\\'),
                "{name} contains an invalid key: {key}"
            );
        }
    }

    fn assert_sorted_and_unique(name: &str, assets: &[Asset]) {
        for pair in assets.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "{name} keys must be sorted and unique: {} then {}",
                pair[0].0,
                pair[1].0
            );
        }
    }
}
