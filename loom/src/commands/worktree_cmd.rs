//! Worktree management commands
//! Usage: loom worktree [list|clean]

use anyhow::Result;
use std::path::PathBuf;

/// List all worktrees
pub fn list() -> Result<()> {
    println!("Git worktrees:");
    println!("─────────────────────────────────────────────────────────");

    let worktrees_dir = std::env::current_dir()?.join(".worktrees");
    if !worktrees_dir.exists() {
        println!("(no .worktrees/ directory)");
        return Ok(());
    }

    if let Ok(entries) = std::fs::read_dir(&worktrees_dir) {
        let mut found = false;
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name();
                let stage_name = name.to_string_lossy();
                let branch = format!("loom/{stage_name}");
                println!("  {stage_name} -> {branch}");
                found = true;
            }
        }
        if !found {
            println!("(no worktrees found)");
        }
    }

    Ok(())
}

/// Clean orphaned worktrees
pub fn clean() -> Result<()> {
    println!("Cleaning orphaned worktrees...");

    let worktrees_dir = std::env::current_dir()?.join(".worktrees");
    if !worktrees_dir.exists() {
        println!("No .worktrees/ directory to clean");
        return Ok(());
    }

    // Would run: git worktree prune
    println!("\nWould run: git worktree prune");
    println!("Note: Full clean requires Phase 4 (git module)");

    Ok(())
}

/// Get the base worktrees directory
pub fn worktrees_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".worktrees")
}
