//! Clean command for loom resource cleanup
//! Usage: loom clean [--all] [--worktrees] [--sessions] [--state]

mod base_graphs;
mod sessions;
mod worktrees;

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use sessions::{clean_sessions, SessionReapMode};
use worktrees::{clean_worktrees, confirm_branch_deletion, run_bare_clean};

/// Statistics for cleanup operations
#[derive(Default)]
struct CleanStats {
    worktrees_removed: usize,
    branches_removed: usize,
    sessions_killed: usize,
    state_removed: bool,
}

/// Execute the clean command
///
/// # Arguments
/// * `all` - Remove all loom resources
/// * `worktrees` - Remove only worktrees
/// * `sessions` - Kill only sessions
/// * `state` - Remove only .work/ state directory
///
/// Bare `loom clean` (no flags) is intentionally NON-destructive: it only
/// prunes stale git worktree references and prints help. The destructive path
/// (deleting worktrees, `loom/*` branches, and `.work/`) requires an explicit
/// flag — `--all`, `--worktrees`, or `--state`. Before any `loom/*` branch with
/// unmerged commits is deleted, the user is shown the commits-ahead counts and
/// asked to confirm (skip the prompt with `LOOM_CLEAN_YES=1`).
pub fn execute(all: bool, worktrees: bool, sessions: bool, state: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;

    // Print header
    print_header();

    // Base graph GC (A.14): runs every invocation — see print_base_graph_section.
    print_base_graph_section(&repo_root);

    // Bare invocation with no flags: do NOT treat as --all. Prune-only + help.
    if !all && !worktrees && !sessions && !state {
        return run_bare_clean(&repo_root);
    }

    let clean_all = all;

    // If we are about to delete worktree branches, surface any unmerged work and
    // require confirmation. This guards a user who typed `loom clean --all`
    // mid-plan from silently losing committed-but-unmerged branches.
    if (clean_all || worktrees) && !confirm_branch_deletion(&repo_root)? {
        println!();
        println!("{} Aborted — nothing was deleted.", "✗".red().bold());
        return Ok(());
    }

    let mut stats = CleanStats::default();

    // Clean worktrees
    if clean_all || worktrees {
        println!("\n{}", "Worktrees".bold());
        println!("{}", "─".repeat(40).dimmed());
        let (wt_count, br_count) = clean_worktrees(&repo_root)?;
        stats.worktrees_removed = wt_count;
        stats.branches_removed = br_count;
    }

    // `.work/` is about to be destroyed on the `--all`/`--state` paths below,
    // which destroys the only record (`.work/sessions/<id>.md`) that lets a
    // tmux socket ever be attributed back to this repo. Reap attributed
    // sockets FIRST in that case, live or not; a bare `--sessions` (with
    // neither `--all` nor `--state`) stays conservative and reaps only dead
    // ones.
    let will_destroy_state = clean_all || state;
    clean_sessions_and_state(&repo_root, sessions, will_destroy_state, &mut stats)?;

    print_summary(&stats);

    Ok(())
}

/// Runs the `--sessions` step and, if `.work/` is about to be destroyed, the
/// `--state` step right after it. Split out of [`execute`] to keep it under
/// the line-count cap; kept as one function (rather than two standalone
/// helpers) because [`SessionReapMode`] selection genuinely depends on
/// `will_destroy_state`, the same flag that gates the state step.
fn clean_sessions_and_state(
    repo_root: &Path,
    sessions: bool,
    will_destroy_state: bool,
    stats: &mut CleanStats,
) -> Result<()> {
    if sessions || will_destroy_state {
        println!("\n{}", "Sessions".bold());
        println!("{}", "─".repeat(40).dimmed());
        let mode = if will_destroy_state {
            SessionReapMode::IncludeLiveBeforeClean
        } else {
            SessionReapMode::OrphansOnly
        };
        stats.sessions_killed = clean_sessions(repo_root, mode)?;
    }

    if will_destroy_state {
        println!("\n{}", "State".bold());
        println!("{}", "─".repeat(40).dimmed());
        stats.state_removed = clean_state_directory(repo_root)?;
    }

    Ok(())
}

/// Print the loom clean header
fn print_header() {
    crate::utils::print_logo_header("Cleaning...");
}

/// Prune stale `graph/base/*.json` layer files and print the result.
/// A prune failure is reported but never aborts the rest of `execute` — base
/// graph GC is a courtesy, not something the rest of `loom clean` depends on.
fn print_base_graph_section(repo_root: &Path) {
    println!("\n{}", "Base graphs".bold());
    println!("{}", "─".repeat(40).dimmed());
    match base_graphs::prune_base_graphs(repo_root) {
        Ok((0, _)) => println!("  {} Nothing to prune", "─".dimmed()),
        Ok((removed, freed_bytes)) => println!(
            "  {} Pruned {} layer file{} ({})",
            "✓".green().bold(),
            removed,
            if removed == 1 { "" } else { "s" },
            base_graphs::human_bytes(freed_bytes)
        ),
        Err(error) => println!(
            "  {} Base graph prune: {}",
            "⚠".yellow().bold(),
            error.to_string().dimmed()
        ),
    }
}

/// Remove the .work/ state directory
///
/// Returns true if the directory was removed
fn clean_state_directory(repo_root: &Path) -> Result<bool> {
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        println!("  {} No {} directory", "─".dimmed(), ".work/".dimmed());
        return Ok(false);
    }

    fs::remove_dir_all(&work_dir).with_context(|| {
        format!(
            "Failed to remove .work/ directory at {}",
            work_dir.display()
        )
    })?;
    println!("  {} Removed {}", "✓".green().bold(), ".work/".dimmed());

    Ok(true)
}

/// Print the final summary
fn print_summary(stats: &CleanStats) {
    println!();
    println!("{}", "═".repeat(40).dimmed());

    let has_cleanup = stats.worktrees_removed > 0
        || stats.branches_removed > 0
        || stats.sessions_killed > 0
        || stats.state_removed;

    if has_cleanup {
        println!("{} Cleanup complete", "✓".green().bold());

        let mut items: Vec<String> = Vec::new();
        if stats.worktrees_removed > 0 {
            items.push(format!(
                "{} worktree{}",
                stats.worktrees_removed,
                if stats.worktrees_removed == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        if stats.branches_removed > 0 {
            items.push(format!(
                "{} branch{}",
                stats.branches_removed,
                if stats.branches_removed == 1 {
                    ""
                } else {
                    "es"
                }
            ));
        }
        if stats.sessions_killed > 0 {
            items.push(format!(
                "{} session{}",
                stats.sessions_killed,
                if stats.sessions_killed == 1 { "" } else { "s" }
            ));
        }
        if stats.state_removed {
            items.push("state directory".to_string());
        }

        println!("  Removed: {}", items.join(", ").dimmed());
    } else {
        println!("{} Nothing to clean", "✓".green().bold());
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_clean_state_directory_when_exists() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir(&work_dir).unwrap();
        fs::write(work_dir.join("test.txt"), "test").unwrap();

        let result = clean_state_directory(temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(!work_dir.exists());
    }

    #[test]
    fn test_clean_state_directory_when_not_exists() {
        let temp_dir = TempDir::new().unwrap();

        let result = clean_state_directory(temp_dir.path());
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
