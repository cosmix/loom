//! Merge completed stage worktree back to main
//! Usage: flux merge <stage_id>

use anyhow::{bail, Result};
use std::path::PathBuf;

/// Merge worktree branch to main, remove worktree on success
pub fn execute(stage_id: String) -> Result<()> {
    println!("Merging stage: {stage_id}");

    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'flux init' first.");
    }

    // Check worktree exists
    let worktree_path = std::env::current_dir()?.join(".worktrees").join(&stage_id);
    if !worktree_path.exists() {
        bail!(
            "Worktree for stage '{stage_id}' not found at {}",
            worktree_path.display()
        );
    }

    println!("Worktree path: {}", worktree_path.display());
    println!("Branch to merge: flux/{stage_id}");
    println!("\nNote: Full merge requires Phase 4 (git module)");
    Ok(())
}

/// Get the worktree path for a stage
pub fn worktree_path(stage_id: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".worktrees")
        .join(stage_id)
}
