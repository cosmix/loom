//! Conservative discovery and cleanup of orphaned stage worktrees.

use super::branch::branch_exists_strict;
use super::{prune_worktrees, CleanupConfig, CleanupResult};
use crate::fs::stage_files::find_stage_file;
use crate::git::branch::{branch_name_for_stage, commits_ahead_of, default_branch, is_ancestor_of};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle};
use crate::verify::transitions::parse_stage_from_markdown;
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

/// Clean orphaned worktrees only when their history is provably retained.
pub fn cleanup_orphaned_worktrees(repo_root: &Path) -> Result<()> {
    println!("Cleaning orphaned worktrees...");
    println!("{}", "─".repeat(50).dimmed());
    let worktrees_dir = repo_root.join(".worktrees");
    if !worktrees_dir.try_exists()? {
        println!("No .worktrees/ directory to clean");
        return Ok(());
    }

    let worktree_ids = collect_worktree_ids(&worktrees_dir)?;
    if worktree_ids.is_empty() {
        println!("No worktrees found");
        return finish_cleanup(repo_root);
    }
    println!(
        "Found {} worktree(s): {}\n",
        worktree_ids.len(),
        worktree_ids.join(", ").dimmed()
    );
    let work_dir = crate::fs::work_dir::WorkDir::new(repo_root)?
        .root()
        .to_path_buf();
    let target = target_branch(&work_dir, repo_root)?;
    let (orphaned, active) = partition_worktrees(&worktree_ids, &target, repo_root, &work_dir)?;
    clean_partition(&orphaned, active, repo_root, &target, &work_dir)?;
    finish_cleanup(repo_root)
}

fn collect_worktree_ids(worktrees_dir: &Path) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(worktrees_dir)
        .with_context(|| format!("Failed to read {}", worktrees_dir.display()))?
    {
        let entry = entry.context("Failed to read worktree directory entry")?;
        if entry.file_type()?.is_dir() {
            ids.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(ids)
}

fn target_branch(work_dir: &Path, repo_root: &Path) -> Result<String> {
    let configured = crate::fs::work_dir::load_config(work_dir)
        .context("Failed to load Loom configuration for orphan cleanup")?
        .and_then(|config| config.base_branch());
    configured.map_or_else(
        || default_branch(repo_root).context("Failed to resolve cleanup target branch"),
        Ok,
    )
}

fn partition_worktrees(
    ids: &[String],
    target: &str,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<(Vec<String>, usize)> {
    let stages_dir = work_dir.join("stages");
    let mut orphaned = Vec::new();
    let mut active = 0;
    for stage_id in ids {
        if is_orphan(stage_id, target, repo_root, &stages_dir)? {
            orphaned.push(stage_id.clone());
        } else {
            active += 1;
        }
    }
    Ok((orphaned, active))
}

fn is_orphan(stage_id: &str, target: &str, repo_root: &Path, stages_dir: &Path) -> Result<bool> {
    let Some(stage_path) = find_stage_file(stages_dir, stage_id)? else {
        let safe = branch_has_no_unmerged_work(stage_id, target, repo_root);
        print_missing_stage_decision(stage_id, safe);
        return Ok(safe);
    };
    let content = match std::fs::read_to_string(&stage_path) {
        Ok(content) => content,
        Err(_) => {
            println!(
                "  {} {} (unreadable stage file — keeping)",
                "keep:".green(),
                stage_id.cyan()
            );
            return Ok(false);
        }
    };
    let stage = match parse_stage_from_markdown(&content) {
        Ok(stage) => stage,
        Err(_) => {
            println!(
                "  {} {} (unparseable stage file — keeping)",
                "keep:".green(),
                stage_id.cyan()
            );
            return Ok(false);
        }
    };
    Ok(classify_stage(stage_id, &stage, target, repo_root))
}

fn print_missing_stage_decision(stage_id: &str, safe: bool) {
    if safe {
        println!(
            "  {} {} (no stage file, branch fully merged)",
            "orphan:".yellow(),
            stage_id.cyan()
        );
    } else {
        println!(
            "  {} {} (no stage file, branch holds unmerged commits — keeping)",
            "keep:".green(),
            stage_id.cyan()
        );
    }
}

fn classify_stage(stage_id: &str, stage: &Stage, target: &str, repo_root: &Path) -> bool {
    let terminal = matches!(stage.status, StageStatus::Skipped)
        || (matches!(stage.status, StageStatus::Completed) && stage.merged);
    if !terminal {
        println!(
            "  {} {} ({})",
            "active:".green(),
            stage_id.cyan(),
            stage.status.to_string().dimmed()
        );
        return false;
    }
    let completed_is_merged = stage
        .completed_commit
        .as_deref()
        .is_some_and(|commit| is_ancestor_of(commit, target, repo_root).unwrap_or(false));
    let branch_is_merged = branch_has_no_unmerged_work(stage_id, target, repo_root);
    let safe =
        branch_is_merged && (completed_is_merged || matches!(stage.status, StageStatus::Skipped));
    let label = if safe {
        "orphan:".yellow()
    } else {
        "keep:".green()
    };
    let reason = if safe {
        "merged"
    } else {
        "unverified merge — keeping"
    };
    println!(
        "  {} {} ({}, {reason})",
        label,
        stage_id.cyan(),
        stage.status.to_string().dimmed()
    );
    safe
}

fn branch_has_no_unmerged_work(stage_id: &str, target: &str, repo_root: &Path) -> bool {
    let branch = branch_name_for_stage(stage_id);
    matches!(branch_exists_strict(&branch, repo_root), Ok(true))
        && matches!(commits_ahead_of(&branch, target, repo_root), Ok(0))
}

fn clean_partition(
    orphaned: &[String],
    active: usize,
    repo_root: &Path,
    target: &str,
    work_dir: &Path,
) -> Result<()> {
    println!();
    if orphaned.is_empty() {
        println!(
            "{} No orphaned worktrees to clean ({active} active)",
            "✓".green().bold()
        );
        return Ok(());
    }
    println!(
        "Cleaning {} orphaned worktree(s)...",
        orphaned.len().to_string().yellow()
    );
    let config = CleanupConfig {
        force_worktree_removal: false,
        force_branch_deletion: true,
        prune_worktrees: false,
        verbose: false,
    };
    let mut failures = Vec::new();
    for stage_id in orphaned {
        let outcome = MergeLifecycle::new(stage_id, repo_root, work_dir).cleanup(target, &config);
        report_orphan_cleanup(stage_id, outcome, &mut failures);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "Failed to clean orphaned worktrees: {}",
            failures.join("; ")
        )
    }
}

/// Dispatch one orphan's `CleanupOutcome` to the existing per-stage output,
/// accumulating a real failure into `failures`. `Deferred`/`Refused` are
/// deliberate refusals, not errors: the orphan partition already proved
/// safety (ancestry + zero unmerged commits) before reaching here, so a
/// refusal just means the primitive could not independently confirm it —
/// skip the stage rather than fail the whole batch.
fn report_orphan_cleanup(stage_id: &str, outcome: CleanupOutcome, failures: &mut Vec<String>) {
    match outcome {
        CleanupOutcome::Done(result) => print_cleanup_result(stage_id, &result),
        CleanupOutcome::NothingToDo => {}
        CleanupOutcome::Deferred => {
            println!("  {} {} (cleanup deferred)", "─".dimmed(), stage_id);
        }
        CleanupOutcome::Refused { reason } => {
            println!("  {} {} (skipped: {reason})", "─".dimmed(), stage_id);
        }
        CleanupOutcome::Failed(error) => {
            println!("  {} {} ({error})", "✗".red().bold(), stage_id);
            failures.push(format!("{stage_id}: {error}"));
        }
    }
}

fn print_cleanup_result(stage_id: &str, result: &CleanupResult) {
    let mut actions = Vec::new();
    if result.worktree_removed {
        actions.push("worktree");
    }
    if result.branch_deleted {
        actions.push("branch");
    }
    if result.base_branch_deleted {
        actions.push("base branch");
    }
    if actions.is_empty() {
        println!("  {} {} (already clean)", "─".dimmed(), stage_id);
    } else {
        println!(
            "  {} {} (removed: {})",
            "✓".green().bold(),
            stage_id,
            actions.join(", ")
        );
    }
}

fn finish_cleanup(repo_root: &Path) -> Result<()> {
    println!();
    prune_worktrees(repo_root)?;
    println!("{} Pruned stale worktree references", "✓".green().bold());
    println!("\n{} Cleanup complete!", "✓".green().bold());
    Ok(())
}
