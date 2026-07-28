//! Knowledge audit command - analyze knowledge files and recommend restructuring.

use crate::fs::knowledge::{KnowledgeDir, KnowledgeLayout};
use crate::fs::work_dir::WorkDir;
use anyhow::{Context, Result};
use colored::Colorize;

pub fn audit(max_file_lines: usize, max_topic_lines: usize, quiet: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    let knowledge = KnowledgeDir::new(project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    let metrics = knowledge.analyze_gc_metrics(max_file_lines, max_topic_lines)?;

    println!("{}", "Knowledge Audit".bold());
    println!();

    let layout_label = match metrics.layout {
        KnowledgeLayout::Hierarchical => "hierarchical",
        KnowledgeLayout::Legacy => "legacy (flat)",
    };
    println!("Layout: {}", layout_label.cyan());
    println!();

    println!("{}", "Tier 1 (summaries):".cyan().bold());
    for tier1 in &metrics.tier1 {
        let icon = if tier1.has_issues {
            "⚠".yellow().to_string()
        } else {
            "─".dimmed().to_string()
        };

        println!(
            "  {} {} ({} lines, {} dups, {} promoted, {} oversized)",
            icon,
            tier1.file_type.filename().cyan(),
            tier1.line_count,
            tier1.duplicate_headers.len(),
            tier1.promoted_block_count,
            tier1.oversized_sections.len(),
        );
        for (heading, lines) in &tier1.oversized_sections {
            println!(
                "      {} '{}' is {} lines — candidate for extraction into a topic",
                "→".dimmed(),
                heading,
                lines
            );
        }
    }

    if !metrics.topics.is_empty() {
        println!();
        println!("{}", "Tier 2 (topics):".cyan().bold());
        for topic in &metrics.topics {
            let icon = if topic.has_issues {
                "⚠".yellow().to_string()
            } else {
                "─".dimmed().to_string()
            };
            let orphan = if topic.is_orphan {
                " [orphan]".red().to_string()
            } else {
                String::new()
            };
            println!(
                "  {} {} ({} lines, {} dups){}",
                icon,
                topic.relative_path().cyan(),
                topic.line_count,
                topic.duplicate_headers.len(),
                orphan,
            );
        }
    }

    let broken_links = metrics.broken_links();
    if !broken_links.is_empty() {
        println!();
        println!("{}", "Broken links:".red().bold());
        for (from, to) in &broken_links {
            println!("  {} {} -> {}", "✗".red(), from, to);
        }
    }

    if metrics.index_stale {
        println!();
        println!(
            "  {} INDEX.md is missing or stale — run '{}'",
            "⚠".yellow(),
            "loom knowledge index".cyan()
        );
    }

    println!();
    println!(
        "Total: {} lines (informational only — there is no aggregate budget)",
        metrics.total_lines
    );
    println!();

    if metrics.gc_recommended {
        println!("Audit result: {}", "GC recommended".yellow().bold());
        for reason in &metrics.reasons {
            println!("  - {}", reason);
        }

        if !quiet {
            println!();
            println!("{}", "Restructuring Instructions:".cyan().bold());
            println!("  1. Extract oversized tier-1 sections into tier-2 topics, replacing them with a 2-4 line summary plus a link");
            println!("  2. Merge duplicate headers into single consolidated sections");
            println!("  3. Repair broken links and adopt orphan topics by linking them from a tier-1 file");
            println!("  4. Delete only genuinely stale content — never a recorded lesson");
            println!("  5. Re-index: run '{}'", "loom knowledge index".cyan());
            println!(
                "  Or: run '{}' to restructure automatically.",
                "loom knowledge gc".cyan()
            );
        }
    } else {
        println!(
            "{}",
            "Knowledge files are clean. No restructuring needed.".green()
        );
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests_audit.rs"]
mod tests;
