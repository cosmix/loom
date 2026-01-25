//! Map command - analyze codebase structure and write to knowledge files.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::fs::work_dir::WorkDir;
use crate::map::{analyze_codebase, AnalysisResult};

/// Execute the map command
pub fn execute(deep: bool, focus: Option<String>, overwrite: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let project_root = work_dir
        .main_project_root()
        .context("Could not determine project root")?;

    println!(
        "{} Mapping codebase{}...",
        "→".cyan().bold(),
        if deep { " (deep mode)" } else { "" }
    );

    // Run analysis
    let result = analyze_codebase(&project_root, deep, focus.as_deref())?;

    // Initialize knowledge if needed
    let knowledge = KnowledgeDir::new(&project_root);
    if !knowledge.exists() {
        knowledge.initialize()?;
    }

    // Write results to knowledge files
    write_analysis_results(&knowledge, &result, overwrite)?;

    println!("\n{} Codebase mapped successfully!", "✓".green().bold());
    println!("  Run 'loom knowledge show' to view results.");

    Ok(())
}

fn write_analysis_results(
    knowledge: &KnowledgeDir,
    result: &AnalysisResult,
    _overwrite: bool,
) -> Result<()> {
    // Write architecture findings
    if !result.architecture.is_empty() {
        println!("  {} architecture.md", "→".cyan());
        knowledge.append(KnowledgeFile::Architecture, &result.architecture)?;
    }

    // Write stack findings
    if !result.stack.is_empty() {
        println!("  {} stack.md", "→".cyan());
        knowledge.append(KnowledgeFile::Stack, &result.stack)?;
    }

    // Write conventions
    if !result.conventions.is_empty() {
        println!("  {} conventions.md", "→".cyan());
        knowledge.append(KnowledgeFile::Conventions, &result.conventions)?;
    }

    // Write concerns
    if !result.concerns.is_empty() {
        println!("  {} concerns.md", "→".cyan());
        knowledge.append(KnowledgeFile::Concerns, &result.concerns)?;
    }

    Ok(())
}
