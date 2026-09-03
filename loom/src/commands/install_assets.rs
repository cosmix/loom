//! Install loom's embedded assets into the Claude and Codex configuration trees.

use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::assets::install::{default_paths, install_all, InstallPaths, InstallReport};
use crate::skills::SkillLayout;

/// Resolve paths, choose the skill layout, install assets, and print a summary.
pub fn execute(
    claude_dir: Option<PathBuf>,
    codex_dir: Option<PathBuf>,
    skills: Option<SkillLayout>,
) -> Result<()> {
    let refresh_completions = claude_dir.is_none() && codex_dir.is_none();
    let claude_dir = match claude_dir {
        Some(dir) => dir,
        None => default_paths()?.claude_dir,
    };
    let codex_dir = match codex_dir {
        Some(dir) => dir,
        None => default_paths()?.codex_dir,
    };
    let paths = InstallPaths {
        claude_dir,
        codex_dir,
    };
    let layout = resolve_layout(&paths.claude_dir, skills);
    let report = install_all(&paths, layout, refresh_completions)?;
    print_summary(&report);
    Ok(())
}

/// Select an explicit layout or preserve the layout of an existing tree.
fn resolve_layout(claude_dir: &Path, skills: Option<SkillLayout>) -> Option<SkillLayout> {
    match skills {
        Some(layout) => Some(layout),
        None if !claude_dir.join("loom-install.toml").exists()
            && !claude_dir.join("skills").exists() =>
        {
            Some(SkillLayout::Core)
        }
        None => None,
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
    println!("  {} hooks updated: {}", "✓".green(), report.hooks);
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
