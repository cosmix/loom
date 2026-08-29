//! The catalogued half of the skill index.
//!
//! `~/.claude/skills/` is scanned by Claude Code itself, and every skill's
//! `description:` line stays resident in every request whether or not the
//! skill is ever invoked. Loom keeps only its own orchestration mechanics
//! there and installs the rest into a sibling directory Claude Code never
//! scans, `~/.claude/loom-skill-catalog/`, loaded on demand by the
//! `loom-skills` loader skill.
//!
//! The manifest of which skills stay resident is `skills/core-skills.txt`,
//! compiled into the binary with `include_str!` so this module and
//! `install.sh` — which reads the same file at install time — cannot drift
//! into two different lists.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::SkillIndex;

/// Directory under ~/.claude/ holding skills Claude Code does not index.
pub const CATALOG_DIR_NAME: &str = "loom-skill-catalog";

const CORE_SKILLS_MANIFEST: &str = include_str!("../../../skills/core-skills.txt");

/// Core skill names, in manifest order.
pub fn core_skill_names() -> impl Iterator<Item = &'static str> + Clone {
    CORE_SKILLS_MANIFEST.lines().filter_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some(trimmed)
        }
    })
}

/// True when `name` is installed into ~/.claude/skills/ rather than catalogued.
pub fn is_core_skill(name: &str) -> bool {
    core_skill_names().any(|core| core == name)
}

/// The Skill-tool invocation that actually loads `name`.
///
/// Decided from the compiled-in manifest rather than the filesystem, so the
/// rendered form is deterministic in tests and stays correct even on a
/// `--skills all` install that has no catalog directory at all — the
/// `loom-skills` loader itself falls back to `~/.claude/skills/<name>/SKILL.md`
/// when there is no catalog to read.
///
/// `SkillIndex::load_from_directory` indexes every subdirectory of
/// `~/.claude/skills` that has a `SKILL.md`, including skills that are not
/// loom's — the catalog split only ever moved loom's own `loom-`prefixed
/// skills out of that directory. Routing a non-`loom-` name through the
/// loader would still resolve (the loader falls back to a direct read), but
/// it bypasses that skill's own `allowed-tools` declaration and adds a hop
/// for a skill that already has a working direct invocation. Only a
/// `loom-`prefixed non-core skill is catalogued, so only that case goes
/// through the loader.
pub fn skill_invocation(name: &str) -> String {
    if is_core_skill(name) || !name.starts_with("loom-") {
        format!("Skill(skill=\"{name}\")")
    } else {
        format!("Skill(skill=\"loom-skills\", args=\"{name}\")")
    }
}

/// ~/.claude/loom-skill-catalog, derived as a sibling of the skills directory.
pub fn catalog_dir_for(skills_dir: &Path) -> PathBuf {
    match skills_dir.parent() {
        Some(parent) => parent.join(CATALOG_DIR_NAME),
        None => PathBuf::from(CATALOG_DIR_NAME),
    }
}

/// Load the index from `skills_dir` and its sibling catalog directory.
pub fn load_with_catalog(skills_dir: &Path) -> Result<SkillIndex> {
    load_from_roots(skills_dir, &catalog_dir_for(skills_dir))
}

/// Load the index from two explicit roots. A skill present in both resolves to the
/// `skills_dir` copy.
pub fn load_from_roots(skills_dir: &Path, catalog_dir: &Path) -> Result<SkillIndex> {
    let mut index = SkillIndex::load_from_directory(skills_dir)?;

    if !catalog_dir.exists() {
        return Ok(index);
    }

    let entries = fs::read_dir(catalog_dir).with_context(|| {
        format!(
            "Failed to read catalog directory: {}",
            catalog_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let skill_dir = entry.path();
        if !skill_dir.is_dir() {
            continue;
        }

        let skill_file = skill_dir.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }

        match SkillIndex::parse_skill_file(&skill_file) {
            Ok(metadata) => {
                if index.get_by_name(&metadata.name).is_none() {
                    index.add_skill(metadata);
                }
            }
            Err(e) => {
                // Match load_from_directory's own failure behaviour: warn and
                // skip rather than aborting the whole load over one skill.
                eprintln!(
                    "Warning: Failed to parse skill file {}: {}",
                    skill_file.display(),
                    e
                );
            }
        }
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Repo root, resolved from the crate directory at compile time.
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("loom/ always has a parent - the repo root")
            .to_path_buf()
    }

    /// A `description:` line beyond this length would regrow the resident
    /// cost the catalog split was meant to shrink.
    const MAX_RESIDENT_DESCRIPTION_LEN: usize = 160;

    fn create_catalog_skill(dir: &Path, name: &str, description: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn every_core_skill_name_has_a_skill_directory() {
        let repo_root = repo_root();
        let names: Vec<&str> = core_skill_names().collect();
        assert_eq!(
            names.len(),
            9,
            "manifest should list exactly 9 core skills, got {names:?}"
        );

        for name in names {
            let skill_file = repo_root.join("skills").join(name).join("SKILL.md");
            assert!(
                skill_file.exists(),
                "core skill '{name}' has no {}",
                skill_file.display()
            );
        }
    }

    #[test]
    fn no_skill_description_exceeds_the_resident_cost_cap() {
        let skills_dir = repo_root().join("skills");
        let mut checked = 0usize;

        for entry in fs::read_dir(&skills_dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !path.is_dir() || !name.starts_with("loom-") {
                continue;
            }
            let skill_file = path.join("SKILL.md");
            let Ok(content) = fs::read_to_string(&skill_file) else {
                continue;
            };
            let Some(line) = first_frontmatter_description(&content) else {
                continue;
            };

            checked += 1;
            let value = line.trim_start_matches("description:").trim();
            assert!(
                !is_block_scalar_indicator(value),
                "{}: description uses a block scalar (`description: {value}`); not allowed \
                 because a block scalar's length cannot be capped by this test",
                skill_file.display()
            );
            assert!(
                line.len() <= MAX_RESIDENT_DESCRIPTION_LEN,
                "{}: description line is {} chars, exceeds cap of {MAX_RESIDENT_DESCRIPTION_LEN}",
                skill_file.display(),
                line.len()
            );
        }

        assert!(
            checked >= 60,
            "expected to check at least 60 skill descriptions, checked {checked}"
        );
    }

    /// True when `value` (the text after `description:`, trimmed) is a YAML
    /// block scalar indicator rather than an inline value. A block scalar's
    /// body lives on the following indented lines, so measuring this line
    /// alone would see only the few-character indicator instead of the real,
    /// unbounded resident description.
    fn is_block_scalar_indicator(value: &str) -> bool {
        matches!(value, "|" | "|-" | "|+" | ">" | ">-" | ">+")
    }

    #[test]
    fn is_block_scalar_indicator_detects_all_variants() {
        for indicator in ["|", "|-", "|+", ">", ">-", ">+"] {
            assert!(
                is_block_scalar_indicator(indicator),
                "{indicator} should be detected as a block scalar indicator"
            );
        }
        assert!(!is_block_scalar_indicator("Rust expertise"));
    }

    /// The first `description:` line inside the YAML frontmatter block
    /// (between the first `---` line and the next `---` line) — ignoring any
    /// `description: |` markers that appear later in a skill's body.
    fn first_frontmatter_description(content: &str) -> Option<String> {
        let mut lines = content.lines();
        if lines.next()?.trim() != "---" {
            return None;
        }
        for line in lines {
            let trimmed = line.trim();
            if trimmed == "---" {
                break;
            }
            if trimmed.starts_with("description:") {
                return Some(trimmed.to_string());
            }
        }
        None
    }

    #[test]
    fn is_core_skill_true_for_manifest_entry() {
        assert!(is_core_skill("loom-usage"));
    }

    #[test]
    fn is_core_skill_false_for_catalogued_skill() {
        assert!(!is_core_skill("loom-rust"));
    }

    #[test]
    fn skill_invocation_core_form() {
        assert_eq!(
            skill_invocation("loom-usage"),
            "Skill(skill=\"loom-usage\")"
        );
    }

    #[test]
    fn skill_invocation_catalogued_form() {
        assert_eq!(
            skill_invocation("loom-rust"),
            "Skill(skill=\"loom-skills\", args=\"loom-rust\")"
        );
    }

    #[test]
    fn skill_invocation_non_loom_skill_is_direct() {
        // A user's own skill is indexed (SkillIndex::load_from_directory scans
        // every SKILL.md under ~/.claude/skills), but the catalog only ever
        // holds loom-prefixed skills, so it must never route through the loader.
        assert_eq!(skill_invocation("my-auth"), "Skill(skill=\"my-auth\")");
    }

    #[test]
    fn catalog_dir_for_is_a_sibling_of_skills_dir() {
        let skills_dir = Path::new("/home/user/.claude/skills");
        assert_eq!(
            catalog_dir_for(skills_dir),
            Path::new("/home/user/.claude/loom-skill-catalog")
        );
    }

    #[test]
    fn load_from_roots_indexes_catalogued_skills() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let catalog_dir = temp.path().join("catalog");
        fs::create_dir_all(&skills_dir).unwrap();
        create_catalog_skill(&catalog_dir, "loom-rust", "Rust expertise");

        let index = load_from_roots(&skills_dir, &catalog_dir).unwrap();

        assert!(index.get_by_name("loom-rust").is_some());
    }

    #[test]
    fn load_from_roots_prefers_skills_dir_copy_on_name_collision() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let catalog_dir = temp.path().join("catalog");
        create_catalog_skill(&skills_dir, "dup", "From skills_dir");
        create_catalog_skill(&catalog_dir, "dup", "From catalog_dir");

        let index = load_from_roots(&skills_dir, &catalog_dir).unwrap();

        let metadata = index.get_by_name("dup").expect("dup should be indexed");
        assert_eq!(metadata.description, "From skills_dir");
    }

    #[test]
    fn load_from_roots_tolerates_missing_catalog_directory() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let index = load_from_roots(&skills_dir, &temp.path().join("no-such-catalog")).unwrap();

        assert!(index.is_empty());
    }
}
