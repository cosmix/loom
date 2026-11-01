//! Repair command for fixing loom workspace issues
//!
//! This command diagnoses and optionally fixes common issues with loom workspaces:
//! - Corrupted .loom/work directory (symlink in main repo)
//! - Missing .gitignore entries
//! - Missing git pre-commit hook
//! - Missing loom reference-transaction hook (the target guard's attestation)

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

/// Issue detected during repair check
#[derive(Debug)]
pub struct RepairIssue {
    pub severity: Severity,
    pub description: String,
    pub fix_description: String,
}

#[derive(Debug)]
pub(crate) struct DetectedIssue {
    severity: Severity,
    description: String,
    fix_description: String,
    action: RepairAction,
}

#[derive(Debug, PartialEq, Eq)]
enum RepairAction {
    Manual,
    Workspace(workspace::WorkspaceFix),
    RewriteLocal,
    CleanSettings,
    ReadDenies,
    IdentityDrift,
    OldSkill(String),
    OldAgent(String),
    SkillRefs,
    CodexSlashTmp,
    PhantomMerge(String),
    IncoherentStage { stage_id: String, reason: String },
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

    crate::utils::print_logo_header("Workspace Repair");

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
        println!(
            "{} No issues found - workspace is healthy!",
            "✓".green().bold()
        );
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

/// Check for all potential issues. Order is observable (`loom repair`'s
/// printed report, and `repair/tests.rs`'s assertions) and MUST be
/// preserved: workspace shape and gitignore, then hooks and settings.json
/// permissions, then the `.claude` settings drift checks, then the
/// `$HOME/.claude` asset checks, then the phantom-merge audit, daemon
/// health, the stale knowledge-directory deny, the `Read(...)` deny rules
/// that make every search prompt, and finally incoherent executing stages.
fn check_all_issues(repo_root: &Path) -> Vec<DetectedIssue> {
    let mut issues = Vec::new();

    issues.extend(workspace::check(repo_root));
    issues.extend(hooks::check(repo_root));
    issues.extend(settings_checks::check(repo_root));
    issues.extend(home_assets::check());
    issues.extend(merge_state::check(repo_root));
    issues.extend(daemon_checks::check_daemon_health(repo_root));
    issues.extend(sandbox_settings::check_stale_knowledge_denies(repo_root));
    issues.extend(sandbox_settings::check_read_denies(repo_root));
    issues.extend(repair_coherence::check_incoherent_executing_stages(
        repo_root,
    ));

    issues
}

/// Attempt to fix detected issues
fn apply_fixes(repo_root: &Path, issues: &[DetectedIssue]) -> Result<RepairResult> {
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

/// Apply the action selected by the detector, independently of its display text.
fn fix_issue(repo_root: &Path, issue: &DetectedIssue) -> Result<bool> {
    match &issue.action {
        RepairAction::Manual => Ok(false),
        RepairAction::Workspace(fix) => fix.apply(repo_root, true),
        RepairAction::RewriteLocal => settings_checks::fix_hooks_local(repo_root).map(|()| true),
        RepairAction::CleanSettings => sandbox_settings::fix_sandbox_settings(repo_root)
            .and_then(|()| settings_checks::fix_hooks_local(repo_root))
            .map(|()| true),
        RepairAction::ReadDenies => sandbox_settings::fix_read_denies(repo_root).map(|()| true),
        RepairAction::IdentityDrift => {
            crate::fs::permissions::scrub_main_repo_settings_identity(repo_root);
            Ok(true)
        }
        RepairAction::OldSkill(name) => home_assets::fix_old_skill(name).map(|()| true),
        RepairAction::OldAgent(name) => home_assets::fix_old_agent(name).map(|()| true),
        RepairAction::SkillRefs => home_assets::fix_settings_skill_refs().map(|()| true),
        RepairAction::CodexSlashTmp => settings_checks::fix_codex_slash_tmp(),
        RepairAction::PhantomMerge(stage) => {
            merge_state::fix_phantom_merge(repo_root, stage).map(|()| true)
        }
        RepairAction::IncoherentStage { stage_id, reason } => {
            repair_coherence::fix_incoherent_executing_stage(repo_root, stage_id, reason)
        }
    }
}

mod daemon_checks;
mod home_assets;
mod hooks;
mod merge_state;
mod repair_coherence;
mod sandbox_settings;
mod settings_checks;
pub mod workspace;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_actions;
#[cfg(test)]
mod tests_hook_drift;
#[cfg(test)]
mod tests_local_keys;
#[cfg(test)]
mod tests_token_denies;
