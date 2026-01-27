//! Repair command for fixing loom workspace issues
//!
//! This command diagnoses and optionally fixes common issues with loom workspaces:
//! - Corrupted .work directory (symlink in main repo)
//! - Missing .gitignore entries
//! - Missing git pre-commit hook

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::fs::work_integrity::{
    check_work_dir_state, is_work_dir_git_ignored, is_worktrees_git_ignored, WorkDirState,
};
use crate::git::{install_pre_commit_hook, is_pre_commit_hook_installed};

/// Issue detected during repair check
#[derive(Debug)]
pub struct RepairIssue {
    pub severity: Severity,
    pub description: String,
    pub fix_description: String,
}

/// Severity of the issue
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

/// Result of repair operation
pub struct RepairResult {
    pub issues_found: usize,
    pub issues_fixed: usize,
    pub issues_failed: usize,
}

/// Execute the repair command
///
/// # Arguments
/// * `fix` - If true, attempt to fix issues. If false, just report (dry-run)
pub fn execute(fix: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;

    println!();
    println!("{}", "╭──────────────────────────────────────╮".cyan());
    println!(
        "{}",
        "│       Loom Workspace Repair          │".cyan().bold()
    );
    println!("{}", "╰──────────────────────────────────────╯".cyan());
    println!();

    if fix {
        println!(
            "{} Running in {} mode - will attempt fixes",
            "→".blue().bold(),
            "FIX".green().bold()
        );
    } else {
        println!(
            "{} Running in {} mode - no changes will be made",
            "→".blue().bold(),
            "DRY-RUN".yellow().bold()
        );
        println!("  Use {} to apply fixes", "--fix".cyan());
    }
    println!();

    // Collect all issues
    let issues = check_all_issues(&repo_root);

    if issues.is_empty() {
        println!("{} No issues found - workspace is healthy!", "✓".green().bold());
        return Ok(());
    }

    // Report issues
    println!("{}", "Issues Detected".bold());
    println!("{}", "─".repeat(40).dimmed());
    println!();

    for (i, issue) in issues.iter().enumerate() {
        let icon = match issue.severity {
            Severity::Critical => "✗".red().bold(),
            Severity::Warning => "!".yellow().bold(),
            Severity::Info => "i".blue().bold(),
        };
        let severity_str = match issue.severity {
            Severity::Critical => format!("{}", issue.severity).red().bold(),
            Severity::Warning => format!("{}", issue.severity).yellow().bold(),
            Severity::Info => format!("{}", issue.severity).blue(),
        };

        println!("{} {} [{}]", icon, issue.description, severity_str);
        println!("  {} {}", "Fix:".dimmed(), issue.fix_description.dimmed());
        if i < issues.len() - 1 {
            println!();
        }
    }

    println!();

    // If fix mode, attempt repairs
    if fix {
        println!("{}", "Applying Fixes".bold());
        println!("{}", "─".repeat(40).dimmed());

        let result = apply_fixes(&repo_root, &issues)?;

        println!();
        println!("{}", "Summary".bold());
        println!("{}", "─".repeat(40).dimmed());
        println!(
            "  Issues found:  {}",
            result.issues_found.to_string().bold()
        );
        println!(
            "  Issues fixed:  {}",
            result.issues_fixed.to_string().green().bold()
        );
        if result.issues_failed > 0 {
            println!(
                "  Issues failed: {}",
                result.issues_failed.to_string().red().bold()
            );
        }
    } else {
        let critical_count = issues
            .iter()
            .filter(|i| i.severity == Severity::Critical)
            .count();
        if critical_count > 0 {
            println!(
                "{} {} critical issue(s) found. Run {} to fix.",
                "!".red().bold(),
                critical_count,
                "loom repair --fix".cyan()
            );
        }
    }

    Ok(())
}

/// Check for all potential issues
fn check_all_issues(repo_root: &Path) -> Vec<RepairIssue> {
    let mut issues = Vec::new();

    // Check 1: .work directory state
    let work_state = check_work_dir_state(repo_root);
    match &work_state {
        WorkDirState::Symlink { target } => {
            issues.push(RepairIssue {
                severity: Severity::Critical,
                description: format!(".work is a symlink (-> {target}) in main repo"),
                fix_description: "Remove symlink and reinitialize".to_string(),
            });
        }
        WorkDirState::Invalid => {
            issues.push(RepairIssue {
                severity: Severity::Critical,
                description: ".work exists but is neither directory nor symlink".to_string(),
                fix_description: "Remove and reinitialize".to_string(),
            });
        }
        _ => {}
    }

    // Check 2: .gitignore has .work
    if !is_work_dir_git_ignored(repo_root) {
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: ".work not found in .gitignore".to_string(),
            fix_description: "Add .work/ and .work to .gitignore".to_string(),
        });
    }

    // Check 3: .gitignore has .worktrees
    if !is_worktrees_git_ignored(repo_root) {
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: ".worktrees not found in .gitignore".to_string(),
            fix_description: "Add .worktrees/ and .worktrees to .gitignore".to_string(),
        });
    }

    // Check 4: Git pre-commit hook installed
    if !is_pre_commit_hook_installed(repo_root) {
        issues.push(RepairIssue {
            severity: Severity::Info,
            description: "Git pre-commit hook not installed".to_string(),
            fix_description: "Install loom pre-commit hook".to_string(),
        });
    }

    issues
}

/// Attempt to fix detected issues
fn apply_fixes(repo_root: &Path, issues: &[RepairIssue]) -> Result<RepairResult> {
    let mut fixed = 0;
    let mut failed = 0;

    for issue in issues {
        match fix_issue(repo_root, issue) {
            Ok(true) => {
                println!("  {} Fixed: {}", "✓".green().bold(), issue.description);
                fixed += 1;
            }
            Ok(false) => {
                println!(
                    "  {} Skipped: {} (no action needed)",
                    "-".dimmed(),
                    issue.description
                );
            }
            Err(e) => {
                println!(
                    "  {} Failed: {} - {}",
                    "✗".red().bold(),
                    issue.description,
                    e
                );
                failed += 1;
            }
        }
    }

    Ok(RepairResult {
        issues_found: issues.len(),
        issues_fixed: fixed,
        issues_failed: failed,
    })
}

/// Fix a single issue
fn fix_issue(repo_root: &Path, issue: &RepairIssue) -> Result<bool> {
    // Match based on description (not ideal, but works for now)
    if issue.description.contains(".work is a symlink") {
        fix_work_symlink(repo_root)?;
        Ok(true)
    } else if issue.description.contains(".work exists but is neither") {
        fix_invalid_work(repo_root)?;
        Ok(true)
    } else if issue.description.contains(".work not found in .gitignore") {
        fix_gitignore_work(repo_root)?;
        Ok(true)
    } else if issue.description.contains(".worktrees not found in .gitignore") {
        fix_gitignore_worktrees(repo_root)?;
        Ok(true)
    } else if issue.description.contains("pre-commit hook not installed") {
        install_pre_commit_hook(repo_root)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Fix corrupted .work symlink in main repo
fn fix_work_symlink(repo_root: &Path) -> Result<()> {
    let work_path = repo_root.join(".work");
    fs::remove_file(&work_path).with_context(|| {
        format!(
            "Failed to remove .work symlink at {}",
            work_path.display()
        )
    })?;
    Ok(())
}

/// Fix invalid .work (neither dir nor symlink)
fn fix_invalid_work(repo_root: &Path) -> Result<()> {
    let work_path = repo_root.join(".work");
    if work_path.is_file() {
        fs::remove_file(&work_path)?;
    } else {
        fs::remove_dir_all(&work_path)?;
    }
    Ok(())
}

/// Add .work entries to .gitignore
fn fix_gitignore_work(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");
    let mut content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    // Add entries if not present
    let has_work_dir = content.lines().any(|l| l.trim() == ".work/");
    let has_work = content.lines().any(|l| l.trim() == ".work");

    if !has_work_dir || !has_work {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str("# loom workspace state\n");
        if !has_work_dir {
            content.push_str(".work/\n");
        }
        if !has_work {
            content.push_str(".work\n");
        }
        fs::write(&gitignore_path, content)?;
    }

    Ok(())
}

/// Add .worktrees entries to .gitignore
fn fix_gitignore_worktrees(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");
    let mut content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    let has_worktrees_dir = content.lines().any(|l| l.trim() == ".worktrees/");
    let has_worktrees = content.lines().any(|l| l.trim() == ".worktrees");

    if !has_worktrees_dir || !has_worktrees {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        if !content.is_empty() && !content.contains("# loom worktrees") {
            content.push('\n');
        }
        if !content.contains("# loom worktrees") {
            content.push_str("# loom worktrees\n");
        }
        if !has_worktrees_dir {
            content.push_str(".worktrees/\n");
        }
        if !has_worktrees {
            content.push_str(".worktrees\n");
        }
        fs::write(&gitignore_path, content)?;
    }

    Ok(())
}
