//! Install loom's embedded assets into the Claude and Codex configuration trees.

use anyhow::{bail, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::assets::install::{default_paths, install_all, InstallPaths, InstallReport};
use crate::skills::SkillLayout;

/// Resolve paths, choose the skill layout, install assets, and print a summary.
pub fn execute(
    claude_dir: Option<PathBuf>,
    codex_dir: Option<PathBuf>,
    skills: Option<String>,
) -> Result<()> {
    let defaults = default_paths()?;
    let refresh_completions = claude_dir.is_none() && codex_dir.is_none();
    let paths = InstallPaths {
        claude_dir: claude_dir.unwrap_or(defaults.claude_dir),
        codex_dir: codex_dir.unwrap_or(defaults.codex_dir),
    };
    let layout = resolve_layout(&paths.claude_dir, skills)?;
    let report = install_all(&paths, layout, refresh_completions)?;
    print_summary(&report);
    Ok(())
}

/// Select an explicit layout or preserve the layout of an existing tree.
fn resolve_layout(claude_dir: &Path, skills: Option<String>) -> Result<Option<SkillLayout>> {
    match skills.as_deref() {
        Some("core") => Ok(Some(SkillLayout::Core)),
        Some("all") => Ok(Some(SkillLayout::All)),
        Some(value) => bail!("Unsupported skill layout: {value}"),
        None if !claude_dir.join("loom-install.toml").exists()
            && !claude_dir.join("skills").exists() =>
        {
            Ok(Some(SkillLayout::Core))
        }
        None => Ok(None),
    }
}

/// Print the installation counts and every backup path created by the placer.
fn print_summary(report: &InstallReport) {
    crate::utils::print_logo_header("Install Assets");
    let layout = match report.layout {
        SkillLayout::Core => "core",
        SkillLayout::All => "all",
    };
    println!("  {} agents: {}", "✓".green(), report.agents);
    println!("  {} commands: {}", "✓".green(), report.commands);
    println!("  {} hooks: {}", "✓".green(), report.hooks);
    println!(
        "  {} Claude skills: {} resident, {} catalogued ({layout})",
        "✓".green(),
        report.skills_resident,
        report.skills_catalogued
    );
    println!(
        "  {} Codex skills: {} resident, {} catalogued ({layout})",
        "✓".green(),
        report.codex_skills_resident,
        report.codex_skills_catalogued
    );
    for backup in &report.backups {
        println!("  {} backup: {}", "✓".green(), backup.display());
    }
}
