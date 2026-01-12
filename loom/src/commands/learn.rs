//! Learn command implementations for recording and listing learnings.
//!
//! Commands:
//! - `loom learn mistake <description>` - Record a mistake
//! - `loom learn pattern <description>` - Record a pattern
//! - `loom learn convention <description>` - Record a convention
//! - `loom learn guidance <description> --human` - Record human guidance
//! - `loom learn list [--category <cat>]` - List learnings

use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::env;

use crate::commands::common::{detect_stage_id, truncate_for_display};
use crate::fs::learnings::{
    append_learning, read_learnings, validate_description, Learning, LearningCategory,
};

/// Get the .work directory, handling worktree symlinks
fn get_work_dir() -> Result<std::path::PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;
    let work_dir = cwd.join(".work");

    if !work_dir.exists() {
        anyhow::bail!(".work directory not found. Run 'loom init' first.");
    }

    Ok(work_dir)
}

/// Record a mistake learning
pub fn mistake(description: String, correction: Option<String>) -> Result<()> {
    validate_description(&description)?;
    if let Some(ref c) = correction {
        validate_description(c)?;
    }

    let work_dir = get_work_dir()?;
    let stage_id = detect_stage_id().unwrap_or_else(|| "manual".to_string());

    let learning = Learning {
        timestamp: Utc::now(),
        stage_id: stage_id.clone(),
        description: description.clone(),
        correction,
        source: None,
    };

    append_learning(&work_dir, LearningCategory::Mistake, &learning)?;

    println!(
        "{} Recorded mistake from stage '{}'",
        "✓".green(),
        stage_id.cyan()
    );
    println!("  {}", truncate_for_display(&description, 60));

    Ok(())
}

/// Record a pattern learning
pub fn pattern(description: String) -> Result<()> {
    validate_description(&description)?;

    let work_dir = get_work_dir()?;
    let stage_id = detect_stage_id().unwrap_or_else(|| "manual".to_string());

    let learning = Learning {
        timestamp: Utc::now(),
        stage_id: stage_id.clone(),
        description: description.clone(),
        correction: None,
        source: None,
    };

    append_learning(&work_dir, LearningCategory::Pattern, &learning)?;

    println!(
        "{} Recorded pattern from stage '{}'",
        "✓".green(),
        stage_id.cyan()
    );
    println!("  {}", truncate_for_display(&description, 60));

    Ok(())
}

/// Record a convention learning
pub fn convention(description: String) -> Result<()> {
    validate_description(&description)?;

    let work_dir = get_work_dir()?;
    let stage_id = detect_stage_id().unwrap_or_else(|| "manual".to_string());

    let learning = Learning {
        timestamp: Utc::now(),
        stage_id: stage_id.clone(),
        description: description.clone(),
        correction: None,
        source: None,
    };

    append_learning(&work_dir, LearningCategory::Convention, &learning)?;

    println!(
        "{} Recorded convention from stage '{}'",
        "✓".green(),
        stage_id.cyan()
    );
    println!("  {}", truncate_for_display(&description, 60));

    Ok(())
}

/// Record human guidance (requires --human flag for safety)
pub fn guidance(description: String, human: bool, source: Option<String>) -> Result<()> {
    if !human {
        anyhow::bail!(
            "Human guidance must be recorded with --human flag to confirm it's from a human operator"
        );
    }

    validate_description(&description)?;

    let work_dir = get_work_dir()?;

    let learning = Learning {
        timestamp: Utc::now(),
        stage_id: "human".to_string(),
        description: description.clone(),
        correction: None,
        source,
    };

    append_learning(&work_dir, LearningCategory::Guidance, &learning)?;

    println!("{} Recorded human guidance", "✓".green());
    println!("  {}", truncate_for_display(&description, 60));

    Ok(())
}

/// List learnings, optionally filtered by category
pub fn list(category: Option<String>) -> Result<()> {
    let work_dir = get_work_dir()?;

    let categories_to_show: Vec<LearningCategory> = match &category {
        Some(cat) => vec![cat.parse()?],
        None => LearningCategory::all().to_vec(),
    };

    let mut total_count = 0;

    for cat in categories_to_show {
        let learnings = read_learnings(&work_dir, cat)?;

        if learnings.is_empty() {
            continue;
        }

        total_count += learnings.len();

        println!("\n{} ({})", cat.display_name().bold(), learnings.len());
        println!("{}", "─".repeat(60));

        for learning in learnings.iter().rev().take(10) {
            let date = learning.timestamp.format("%Y-%m-%d").to_string();
            let stage = &learning.stage_id;

            println!(
                "{} {} {}",
                date.dimmed(),
                format!("[{stage}]").cyan(),
                truncate_for_display(&learning.description, 50)
            );

            if let Some(correction) = &learning.correction {
                println!(
                    "  {} {}",
                    "→".dimmed(),
                    truncate_for_display(correction, 48).yellow()
                );
            }
        }

        if learnings.len() > 10 {
            println!("  {} {} more...", "...".dimmed(), learnings.len() - 10);
        }
    }

    if total_count == 0 {
        if category.is_some() {
            println!("{} No learnings found for that category", "ℹ".blue());
        } else {
            println!("{} No learnings recorded yet", "ℹ".blue());
        }
        println!(
            "  Record with: {} <description>",
            "loom learn <category>".cyan()
        );
    }

    Ok(())
}
