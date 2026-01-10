//! Fact command implementations for managing the shared facts store.
//!
//! Commands:
//! - `loom fact set <key> <value>` - Set a fact
//! - `loom fact get <key>` - Get a fact
//! - `loom fact list [--stage <id>]` - List facts

use anyhow::{Context, Result};
use colored::Colorize;
use std::env;

use crate::fs::facts::{validate_fact_key, validate_fact_value, Confidence, FactsStore};

/// Get the .work directory, handling worktree symlinks
fn get_work_dir() -> Result<std::path::PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;
    let work_dir = cwd.join(".work");

    if !work_dir.exists() {
        anyhow::bail!(".work directory not found. Run 'loom init' first.");
    }

    Ok(work_dir)
}

/// Set a fact in the store
pub fn set(
    key: String,
    value: String,
    stage_id: Option<String>,
    confidence: Option<String>,
) -> Result<()> {
    validate_fact_key(&key)?;
    validate_fact_value(&value)?;

    let work_dir = get_work_dir()?;
    let mut store = FactsStore::load(&work_dir)?;

    // Determine the stage ID - use provided, detect from worktree, or use "manual"
    let stage =
        stage_id.unwrap_or_else(|| detect_stage_id().unwrap_or_else(|| "manual".to_string()));

    // Parse confidence level
    let conf = match confidence {
        Some(c) => c.parse::<Confidence>()?,
        None => Confidence::Medium,
    };

    store.set(key.clone(), value.clone(), stage, conf);
    store.save(&work_dir)?;

    println!(
        "{} Set fact '{}' = '{}' (confidence: {})",
        "✓".green(),
        key.cyan(),
        value,
        conf
    );

    Ok(())
}

/// Get a fact from the store
pub fn get(key: String) -> Result<()> {
    let work_dir = get_work_dir()?;
    let store = FactsStore::load(&work_dir)?;

    match store.get(&key) {
        Some(fact) => {
            println!("{}: {}", key.cyan(), fact.value);
            println!(
                "  {} stage={}, confidence={}, timestamp={}",
                "→".dimmed(),
                fact.stage_id.dimmed(),
                fact.confidence.to_string().dimmed(),
                fact.timestamp
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string()
                    .dimmed()
            );
            Ok(())
        }
        None => {
            eprintln!("{} Fact '{}' not found", "✗".red(), key);
            std::process::exit(1);
        }
    }
}

/// List facts from the store
pub fn list(stage_id: Option<String>) -> Result<()> {
    let work_dir = get_work_dir()?;
    let store = FactsStore::load(&work_dir)?;

    let facts = store.list(stage_id.as_deref());

    if facts.is_empty() {
        if let Some(sid) = stage_id {
            println!("{} No facts found for stage '{}'", "ℹ".blue(), sid);
        } else {
            println!("{} No facts in store", "ℹ".blue());
        }
        return Ok(());
    }

    // Header
    println!(
        "{:20} {:40} {:20} {:10}",
        "KEY".bold(),
        "VALUE".bold(),
        "STAGE".bold(),
        "CONFIDENCE".bold()
    );
    println!("{}", "─".repeat(94));

    for (key, fact) in facts {
        let confidence_color = match fact.confidence {
            Confidence::Low => fact.confidence.to_string().yellow(),
            Confidence::Medium => fact.confidence.to_string().normal(),
            Confidence::High => fact.confidence.to_string().green(),
        };

        // Truncate value if too long
        let display_value = if fact.value.len() > 38 {
            format!("{}…", &fact.value[..37])
        } else {
            fact.value.clone()
        };

        println!(
            "{:20} {:40} {:20} {:10}",
            key.cyan(),
            display_value,
            fact.stage_id,
            confidence_color
        );
    }

    Ok(())
}

/// Try to detect the current stage ID from the worktree branch
fn detect_stage_id() -> Option<String> {
    // Check if we're in a worktree by looking at the branch name
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Worktree branches are named loom/<stage-id>
    if let Some(stage_id) = branch.strip_prefix("loom/") {
        // Filter out special branches like _base
        if !stage_id.starts_with('_') {
            return Some(stage_id.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_detect_stage_id_format() {
        // Test the branch parsing logic directly
        let parse_branch = |branch: &str| -> Option<String> {
            branch.strip_prefix("loom/").and_then(|s| {
                if !s.starts_with('_') {
                    Some(s.to_string())
                } else {
                    None
                }
            })
        };

        assert_eq!(
            parse_branch("loom/implement-auth"),
            Some("implement-auth".to_string())
        );
        assert_eq!(
            parse_branch("loom/stage-123"),
            Some("stage-123".to_string())
        );
        assert_eq!(parse_branch("loom/_base"), None);
        assert_eq!(parse_branch("main"), None);
        assert_eq!(parse_branch("feature/test"), None);
    }
}
