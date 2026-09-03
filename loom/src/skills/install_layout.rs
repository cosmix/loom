//! Records the skill layout chosen during installation and reads it later.
//!
//! [`SkillLayout`] represents whether only loom's core skills or all skills
//! are indexed. [`SkillLayout::read`] reads the recorded choice from
//! `~/.claude/loom-install.toml`, falling back to the filesystem when no valid
//! choice is recorded.

use std::fs;
use std::path::Path;

use clap::ValueEnum;

use super::index_catalog::CATALOG_DIR_NAME;

/// Which skill layout an install chose, recorded in ~/.claude/loom-install.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SkillLayout {
    /// Only the loom mechanics skills are indexed; the rest live in the catalog.
    Core,
    /// Every skill is indexed; there is no catalog directory.
    All,
}

impl SkillLayout {
    /// Read `skills = "core" | "all"` from `<claude_dir>/loom-install.toml`.
    ///
    /// A missing, unreadable, or malformed file — or one with no `skills`
    /// key — infers the layout from the filesystem instead of guessing a
    /// constant, so a machine that predates the `--skills` flag keeps
    /// whatever layout it already has rather than being silently rewritten.
    pub fn read(claude_dir: &Path) -> Self {
        let recorded = fs::read_to_string(claude_dir.join("loom-install.toml"))
            .ok()
            .and_then(|content| toml::from_str::<toml::Value>(&content).ok())
            .and_then(|value| {
                value
                    .get("skills")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            });

        match recorded.as_deref() {
            Some("core") => SkillLayout::Core,
            Some("all") => SkillLayout::All,
            _ => Self::infer(claude_dir),
        }
    }

    /// Infer the layout from the filesystem when nothing was recorded.
    fn infer(claude_dir: &Path) -> Self {
        if claude_dir.join(CATALOG_DIR_NAME).is_dir() {
            SkillLayout::Core
        } else {
            SkillLayout::All
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_install_toml(claude_dir: &Path, skills: &str) {
        fs::write(
            claude_dir.join("loom-install.toml"),
            format!("skills = \"{skills}\"\n"),
        )
        .unwrap();
    }

    #[test]
    fn read_returns_core_when_recorded() {
        let temp = TempDir::new().unwrap();
        write_install_toml(temp.path(), "core");
        assert_eq!(SkillLayout::read(temp.path()), SkillLayout::Core);
    }

    #[test]
    fn read_returns_all_when_recorded() {
        let temp = TempDir::new().unwrap();
        write_install_toml(temp.path(), "all");
        assert_eq!(SkillLayout::read(temp.path()), SkillLayout::All);
    }

    #[test]
    fn read_infers_core_when_file_missing_but_catalog_present() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(CATALOG_DIR_NAME)).unwrap();
        assert_eq!(SkillLayout::read(temp.path()), SkillLayout::Core);
    }

    #[test]
    fn read_infers_all_when_file_missing_and_no_catalog() {
        let temp = TempDir::new().unwrap();
        assert_eq!(SkillLayout::read(temp.path()), SkillLayout::All);
    }

    #[test]
    fn read_infers_from_filesystem_when_file_is_malformed() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("loom-install.toml"), "not valid toml {{{").unwrap();
        fs::create_dir_all(temp.path().join(CATALOG_DIR_NAME)).unwrap();
        assert_eq!(SkillLayout::read(temp.path()), SkillLayout::Core);
    }
}
