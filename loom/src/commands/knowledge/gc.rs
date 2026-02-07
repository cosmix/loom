//! Knowledge GC command - analyze knowledge files and recommend compaction.

use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use anyhow::{Context, Result};
use colored::Colorize;

pub fn gc(max_file_lines: usize, max_total_lines: usize, quiet: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(main_project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    let metrics = knowledge.analyze_gc_metrics(max_file_lines, max_total_lines)?;

    println!("{}", "Knowledge GC Analysis".bold());
    println!();

    println!("{}", "Files:".cyan().bold());
    for file_metric in &metrics.per_file {
        let icon = if file_metric.has_issues {
            "⚠".yellow().to_string()
        } else {
            "─".dimmed().to_string()
        };

        println!(
            "  {} {} ({} lines, {} dups, {} promoted)",
            icon,
            file_metric.file_type.filename().cyan(),
            file_metric.line_count,
            file_metric.duplicate_headers.len(),
            file_metric.promoted_block_count,
        );
    }

    println!();
    println!("Total: {} lines", metrics.total_lines);
    println!();

    if metrics.gc_recommended {
        println!("GC recommended: {}", "YES".yellow().bold());
        for reason in &metrics.reasons {
            println!("  - {}", reason);
        }

        if !quiet {
            println!();
            println!("{}", "Compaction Instructions:".cyan().bold());
            println!("  1. Review each knowledge file for outdated or redundant content");
            println!("  2. Merge duplicate headers into single consolidated sections");
            println!("  3. Summarize curated memory blocks into concise knowledge");
            println!("  4. Remove any content that is no longer accurate");
            println!("  5. Edit files directly in doc/loom/knowledge/");
        }
    } else {
        println!(
            "{}",
            "Knowledge files are clean. No compaction needed.".green()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();
        (temp_dir, test_dir)
    }

    #[test]
    #[serial]
    fn test_gc_clean() {
        let (_temp_dir, test_dir) = setup_test_env();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        crate::commands::knowledge::init().expect("Failed to init knowledge");
        crate::commands::knowledge::update(
            "architecture".to_string(),
            Some("## Overview\n\nSmall content".to_string()),
        )
        .expect("Failed to update");

        let result = gc(200, 800, true);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_gc_large_file() {
        let (_temp_dir, test_dir) = setup_test_env();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        crate::commands::knowledge::init().expect("Failed to init knowledge");

        let mut big_content = String::from("## Big Section\n\n");
        for i in 0..250 {
            big_content.push_str(&format!("- Line {}\n", i));
        }
        crate::commands::knowledge::update("architecture".to_string(), Some(big_content))
            .expect("Failed to update");

        let result = gc(200, 800, true);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }
}
