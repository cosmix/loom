//! Where the installer placed loom's skills, and how to put them back that
//! way after `loom self-update` extracts a fresh copy of every skill.
//!
//! `install.sh` extracts the full skill set into `~/.claude/skills/` and,
//! for a `--skills core` install, moves the non-mechanics skills out to a
//! sibling catalog directory it creates. `loom self-update` re-extracts the
//! `skills.zip` release asset into `~/.claude/skills/` on every run, which
//! would silently undo that move — this module restores the layout the
//! original install chose.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::ValueEnum;
use colored::Colorize;

use super::index_catalog::{is_core_skill, CATALOG_DIR_NAME};

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

/// Place the skills under `<claude_dir>` according to the recorded layout.
///
/// Runs immediately after `loom self-update` extracts `skills.zip` into
/// `<claude_dir>/skills`, so at entry that directory holds every skill.
pub fn apply_install_layout(claude_dir: &Path) -> Result<()> {
    let layout = SkillLayout::read(claude_dir);
    let skills_dir = claude_dir.join("skills");
    let catalog_dir = claude_dir.join(CATALOG_DIR_NAME);

    let (installed, catalogued) = match layout {
        SkillLayout::All => {
            restore_all(&skills_dir, &catalog_dir)?;
            (count_loom_dirs(&skills_dir)?, 0)
        }
        SkillLayout::Core => {
            split_core(&skills_dir, &catalog_dir)?;
            (
                count_loom_dirs(&skills_dir)?,
                count_loom_dirs(&catalog_dir)?,
            )
        }
    };

    println!(
        "  {} skills/ updated ({installed} indexed, {catalogued} catalogued)",
        "✓".green()
    );
    Ok(())
}

/// `All` layout: move every catalogued skill back into `skills/`, then drop
/// the catalog directory so the two layouts never coexist.
fn restore_all(skills_dir: &Path, catalog_dir: &Path) -> Result<()> {
    if !catalog_dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(catalog_dir)
        .with_context(|| format!("Failed to read {}", catalog_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        let Some(name) = loom_skill_name(&path) else {
            continue;
        };
        move_dir(&path, &skills_dir.join(&name))?;
    }

    fs::remove_dir_all(catalog_dir)
        .with_context(|| format!("Failed to remove {}", catalog_dir.display()))
}

/// `Core` layout: move non-core `loom-*` skills out of `skills/` into the
/// catalog, and pull any core skill left behind in the catalog back in.
fn split_core(skills_dir: &Path, catalog_dir: &Path) -> Result<()> {
    fs::create_dir_all(catalog_dir)
        .with_context(|| format!("Failed to create {}", catalog_dir.display()))?;

    for entry in fs::read_dir(skills_dir)
        .with_context(|| format!("Failed to read {}", skills_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        let Some(name) = loom_skill_name(&path) else {
            continue;
        };
        if !is_core_skill(&name) {
            move_dir(&path, &catalog_dir.join(&name))?;
        }
    }

    for entry in fs::read_dir(catalog_dir)
        .with_context(|| format!("Failed to read {}", catalog_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        let Some(name) = loom_skill_name(&path) else {
            continue;
        };
        if is_core_skill(&name) {
            move_dir(&path, &skills_dir.join(&name))?;
        }
    }

    Ok(())
}

/// The skill name when `path` is a `loom-*` directory, `None` otherwise.
///
/// Only `loom-*` directories under `~/.claude/skills/` are ever touched: that
/// directory is shared with the user's own skills, and moving or removing one
/// would be unrecoverable data loss. The catalog directory carries no such
/// guarantee — it is loom's own namespace, so `restore_all` removes it
/// wholesale after pulling every `loom-*` entry back out of it.
fn loom_skill_name(path: &Path) -> Option<String> {
    if !path.is_dir() {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    name.starts_with("loom-").then(|| name.to_string())
}

/// Move `src` to `dest`, removing any existing directory at `dest` first so
/// a move never leaves a stale copy behind at the destination.
fn move_dir(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        fs::remove_dir_all(dest)
            .with_context(|| format!("Failed to remove stale {}", dest.display()))?;
    }
    fs::rename(src, dest)
        .with_context(|| format!("Failed to move {} to {}", src.display(), dest.display()))
}

/// Count `loom-*` directories directly under `dir` (0 when `dir` is absent).
fn count_loom_dirs(dir: &Path) -> Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let count = fs::read_dir(dir)
        .with_context(|| format!("Failed to read {}", dir.display()))?
        .flatten()
        .filter(|entry| loom_skill_name(&entry.path()).is_some())
        .count();
    Ok(count)
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

    fn make_skill_dir(parent: &Path, name: &str) {
        let dir = parent.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
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

    #[test]
    fn apply_install_layout_core_splits_noncore_skills_into_catalog() {
        let temp = TempDir::new().unwrap();
        let claude_dir = temp.path();
        write_install_toml(claude_dir, "core");
        let skills_dir = claude_dir.join("skills");
        make_skill_dir(&skills_dir, "loom-plan-writer"); // core
        make_skill_dir(&skills_dir, "loom-rust"); // catalogued
        make_skill_dir(&skills_dir, "my-custom-skill"); // not loom's to touch

        apply_install_layout(claude_dir).unwrap();

        assert!(skills_dir.join("loom-plan-writer").is_dir());
        assert!(!skills_dir.join("loom-rust").exists());
        assert!(claude_dir.join(CATALOG_DIR_NAME).join("loom-rust").is_dir());
        assert!(
            skills_dir.join("my-custom-skill").is_dir(),
            "a non-loom skill directory must be left untouched"
        );
    }

    #[test]
    fn apply_install_layout_core_pulls_core_skill_back_from_catalog() {
        let temp = TempDir::new().unwrap();
        let claude_dir = temp.path();
        write_install_toml(claude_dir, "core");
        let skills_dir = claude_dir.join("skills");
        let catalog_dir = claude_dir.join(CATALOG_DIR_NAME);
        // A stale catalog entry for a skill the current manifest calls core.
        make_skill_dir(&skills_dir, "loom-plan-writer");
        make_skill_dir(&catalog_dir, "loom-plan-writer");

        apply_install_layout(claude_dir).unwrap();

        assert!(skills_dir.join("loom-plan-writer").is_dir());
        assert!(!catalog_dir.join("loom-plan-writer").exists());
    }

    #[test]
    fn apply_install_layout_all_restores_catalog_and_removes_it() {
        let temp = TempDir::new().unwrap();
        let claude_dir = temp.path();
        write_install_toml(claude_dir, "all");
        let skills_dir = claude_dir.join("skills");
        let catalog_dir = claude_dir.join(CATALOG_DIR_NAME);
        make_skill_dir(&skills_dir, "loom-plan-writer");
        make_skill_dir(&catalog_dir, "loom-rust");

        apply_install_layout(claude_dir).unwrap();

        assert!(skills_dir.join("loom-rust").is_dir());
        assert!(!catalog_dir.exists());
    }
}
