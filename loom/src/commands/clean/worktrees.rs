//! Worktree- and branch-cleanup helpers for `loom clean`.
//!
//! Split out of `commands::clean` to keep that module's file under the
//! 400-line cap: this owns everything that touches git worktrees and
//! `loom/*` branches, while `sessions.rs` owns tmux session reaping.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::Path;

use crate::git::branch::{commits_ahead_of, list_loom_branches, resolve_target_branch};
use crate::git::cleanup::{
    cleanup_all_base_branches, cleanup_multiple_stages, prune_worktrees, CleanupConfig,
};

/// Non-destructive default for a flagless `loom clean`.
///
/// Only prunes stale git worktree references (which cannot lose committed
/// work) and prints the explicit flags needed for the destructive paths.
pub(super) fn run_bare_clean(repo_root: &Path) -> Result<()> {
    println!("\n{}", "Prune".bold());
    println!("{}", "─".repeat(40).dimmed());
    match prune_worktrees(repo_root) {
        Ok(()) => println!("  {} Stale worktree references pruned", "✓".green().bold()),
        Err(e) => println!(
            "  {} Worktree prune: {}",
            "⚠".yellow().bold(),
            e.to_string().dimmed()
        ),
    }

    println!();
    println!(
        "{} Bare `loom clean` only prunes stale references.",
        "ℹ".cyan().bold()
    );
    println!("  To delete resources, pass an explicit flag:");
    println!(
        "    {}  remove worktrees + their loom/* branches",
        "loom clean --worktrees".dimmed()
    );
    println!(
        "    {}    remove the .work/ state directory",
        "loom clean --state".dimmed()
    );
    println!(
        "    {}      remove everything (worktrees, branches, state)",
        "loom clean --all".dimmed()
    );
    println!(
        "  {} You will be asked to confirm before unmerged loom/* branches are deleted.",
        "─".dimmed()
    );
    println!();
    Ok(())
}

/// Show any `loom/*` branches that carry unmerged commits and require the user
/// to confirm before they are deleted.
///
/// Returns `Ok(true)` when it is safe to proceed:
/// - no `loom/*` branches have unmerged work (nothing to lose), or
/// - the user typed `y`, or `LOOM_CLEAN_YES=1` was set.
///
/// Returns `Ok(false)` to abort (user declined, or non-interactive stdin with
/// unmerged work and no `LOOM_CLEAN_YES`).
pub(super) fn confirm_branch_deletion(repo_root: &Path) -> Result<bool> {
    // Resolve the merge target so commits-ahead is measured against the right base.
    let work_dir = repo_root.join(".work");
    let config_branch = crate::fs::work_dir::load_config(&work_dir)
        .ok()
        .flatten()
        .and_then(|c| c.base_branch());
    let target_branch = resolve_target_branch(&config_branch, repo_root);

    // Collect loom/* branches with unmerged commits (commits ahead of target).
    let branches = list_loom_branches(repo_root).unwrap_or_default();
    let mut unmerged: Vec<(String, usize)> = Vec::new();
    for branch in &branches {
        // Fail closed: a git error here counts as "has unmerged work" so we warn
        // rather than silently delete.
        let ahead = commits_ahead_of(branch, &target_branch, repo_root).unwrap_or(1);
        if ahead > 0 {
            unmerged.push((branch.clone(), ahead));
        }
    }

    if unmerged.is_empty() {
        // Nothing committed-but-unmerged would be lost.
        return Ok(true);
    }

    println!();
    println!(
        "{} The following {} loom branch(es) have UNMERGED commits and will be deleted:",
        "⚠".yellow().bold(),
        unmerged.len()
    );
    for (branch, ahead) in &unmerged {
        println!(
            "    {} {} ({} commit{} ahead of {})",
            "•".yellow(),
            branch.cyan(),
            ahead,
            if *ahead == 1 { "" } else { "s" },
            target_branch.dimmed()
        );
    }
    println!(
        "  {} This work exists only on these branches and is not on {}.",
        "─".dimmed(),
        target_branch.dimmed()
    );

    // Non-interactive escape hatch (CI / scripts).
    if std::env::var("LOOM_CLEAN_YES").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        println!(
            "  {} LOOM_CLEAN_YES set — proceeding without prompt.",
            "─".dimmed()
        );
        return Ok(true);
    }

    if !std::io::stdin().is_terminal() {
        println!();
        println!(
            "{} Refusing to delete unmerged branches non-interactively. \
             Re-run with LOOM_CLEAN_YES=1 to override.",
            "✗".red().bold()
        );
        return Ok(false);
    }

    print!("\nDelete these unmerged branches and their worktrees? (y/N): ");
    std::io::stdout().flush().ok();
    let mut response = String::new();
    std::io::stdin().read_line(&mut response)?;
    Ok(response.trim().eq_ignore_ascii_case("y"))
}

/// Clean up all loom worktrees and their branches
///
/// Returns (worktrees_removed, branches_removed) counts
pub(super) fn clean_worktrees(repo_root: &Path) -> Result<(usize, usize)> {
    let worktrees_dir = repo_root.join(".worktrees");

    // First, always prune stale git worktrees
    match prune_worktrees(repo_root) {
        Ok(()) => {
            println!("  {} Stale worktrees pruned", "✓".green().bold());
        }
        Err(e) => {
            println!(
                "  {} Worktree prune: {}",
                "⚠".yellow().bold(),
                e.to_string().dimmed()
            );
        }
    }

    if !worktrees_dir.exists() {
        println!("  {} No {} directory", "─".dimmed(), ".worktrees/".dimmed());
        return Ok((0, 0));
    }

    // Collect all stage IDs from .worktrees/ directory
    let mut stage_ids = Vec::new();
    if let Ok(entries) = fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let stage_id = entry.file_name().to_string_lossy().to_string();
                stage_ids.push(stage_id);
            }
        }
    }

    // Use shared cleanup utilities for batch cleanup
    let config = CleanupConfig::forced();
    let stage_id_refs: Vec<&str> = stage_ids.iter().map(|s| s.as_str()).collect();
    let results = cleanup_multiple_stages(&stage_id_refs, repo_root, &config);

    // Count successes and print results
    let mut worktrees_removed = 0;
    let mut branches_removed = 0;

    for (stage_id, result) in results {
        if result.worktree_removed {
            println!(
                "  {} Removed worktree: {}",
                "✓".green().bold(),
                stage_id.dimmed()
            );
            worktrees_removed += 1;
        }

        if result.branch_deleted {
            println!(
                "  {} Deleted branch: {}",
                "✓".green().bold(),
                format!("loom/{stage_id}").dimmed()
            );
            branches_removed += 1;
        }

        // Print warnings if any
        for warning in &result.warnings {
            println!(
                "  {} {}: {}",
                "⚠".yellow().bold(),
                stage_id,
                warning.dimmed()
            );
        }
    }

    // Clean up base branches as well
    match cleanup_all_base_branches(repo_root) {
        Ok(deleted) => {
            for branch in deleted {
                println!(
                    "  {} Deleted base branch: {}",
                    "✓".green().bold(),
                    branch.dimmed()
                );
            }
        }
        Err(e) => {
            println!(
                "  {} Failed to clean base branches: {}",
                "⚠".yellow().bold(),
                e.to_string().dimmed()
            );
        }
    }

    // Remove the .worktrees directory itself if it still exists
    if worktrees_dir.exists() {
        fs::remove_dir_all(&worktrees_dir).with_context(|| {
            format!(
                "Failed to remove .worktrees/ directory at {}",
                worktrees_dir.display()
            )
        })?;
        println!(
            "  {} Removed {}",
            "✓".green().bold(),
            ".worktrees/".dimmed()
        );
    }

    Ok((worktrees_removed, branches_removed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn test_clean_worktrees_when_no_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize as a git repo so git commands don't fail
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let result = clean_worktrees(temp_dir.path());
        assert!(result.is_ok());
        // Returns (0, 0) when no .worktrees directory exists
        assert_eq!(result.unwrap(), (0, 0));
    }
}
