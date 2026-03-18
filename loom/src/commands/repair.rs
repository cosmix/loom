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
use crate::sandbox;

/// Loom-specific skill names referenced in settings.json that may need prefix migration.
const LOOM_SKILL_NAMES: &[&str] = &[
    "accessibility",
    "api-design",
    "api-documentation",
    "argocd",
    "auth",
    "background-jobs",
    "before-after",
    "caching",
    "ci-cd",
    "code-migration",
    "code-review",
    "concurrency",
    "crossplane",
    "data-validation",
    "data-visualization",
    "database-design",
    "dead-code-check",
    "debugging",
    "dependency-scan",
    "diagramming",
    "docker",
    "documentation",
    "e2e-testing",
    "error-handling",
    "event-driven",
    "feature-flags",
    "fluxcd",
    "git-workflow",
    "golang",
    "grafana",
    "i18n",
    "istio",
    "karpenter",
    "kubernetes",
    "kustomize",
    "logging-observability",
    "md-tables",
    "model-evaluation",
    "performance-testing",
    "prometheus",
    "prompt-engineering",
    "python",
    "rate-limiting",
    "react",
    "refactoring",
    "rust",
    "search",
    "security-audit",
    "security-scan",
    "serialization",
    "sql-optimization",
    "technical-writing",
    "terraform",
    "test-strategy",
    "testing",
    "threat-model",
    "typescript",
    "webhooks",
    "wiring-test",
];

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

    // Check 5: Claude Code hooks installed
    {
        let settings_path = repo_root.join(".claude/settings.json");
        let has_hooks = if settings_path.exists() {
            std::fs::read_to_string(&settings_path)
                .ok()
                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                .and_then(|v| v.get("hooks").cloned())
                .is_some()
        } else {
            false
        };
        if !has_hooks {
            issues.push(RepairIssue {
                severity: Severity::Info,
                description: "Claude Code hooks not configured".to_string(),
                fix_description: "Install loom hooks to .claude/settings.json".to_string(),
            });
        }
    }

    // Check 6: Sandbox settings.local.json exists
    {
        let settings_local = repo_root.join(".claude/settings.local.json");
        if !settings_local.exists() {
            issues.push(RepairIssue {
                severity: Severity::Info,
                description: "Sandbox settings not found (.claude/settings.local.json)".to_string(),
                fix_description: "Apply default sandbox settings".to_string(),
            });
        }
    }

    // Check 7: Old unprefixed skills that have a loom- counterpart
    if let Some(home) = dirs::home_dir() {
        let skills_dir = home.join(".claude/skills");
        if skills_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("loom-") || !entry.path().is_dir() {
                        continue;
                    }
                    let prefixed = skills_dir.join(format!("loom-{}", name));
                    if prefixed.is_dir() {
                        issues.push(RepairIssue {
                            severity: Severity::Warning,
                            description: format!(
                                "Old unprefixed skill '{}' found (superseded by 'loom-{}')",
                                name, name
                            ),
                            fix_description: format!(
                                "Remove ~/.claude/skills/{} (loom-{} already installed)",
                                name, name
                            ),
                        });
                    }
                }
            }
        }
    }

    // Check 8: Old unprefixed agents that have a loom- counterpart
    if let Some(home) = dirs::home_dir() {
        let agents_dir = home.join(".claude/agents");
        if agents_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&agents_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("loom-") || !name.ends_with(".md") {
                        continue;
                    }
                    let prefixed = agents_dir.join(format!("loom-{}", name));
                    if prefixed.exists() {
                        let bare = name.trim_end_matches(".md");
                        issues.push(RepairIssue {
                            severity: Severity::Warning,
                            description: format!(
                                "Old unprefixed agent '{}' found (superseded by 'loom-{}')",
                                bare, bare
                            ),
                            fix_description: format!(
                                "Remove ~/.claude/agents/{} (loom-{} already installed)",
                                name, name
                            ),
                        });
                    }
                }
            }
        }
    }

    // Check 10: Settings.json references old-style skill names
    if let Some(home) = dirs::home_dir() {
        let settings_path = home.join(".claude/settings.json");
        if settings_path.exists() {
            if let Ok(content) = fs::read_to_string(&settings_path) {
                let has_old_refs = LOOM_SKILL_NAMES
                    .iter()
                    .any(|name| content.contains(&format!("Skill({}", name)));
                if has_old_refs {
                    issues.push(RepairIssue {
                        severity: Severity::Info,
                        description: "Settings.json references old-style skill names".to_string(),
                        fix_description:
                            "Update skill references from 'name' to 'loom-name' in settings"
                                .to_string(),
                    });
                }
            }
        }
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
    } else if issue
        .description
        .contains(".worktrees not found in .gitignore")
    {
        fix_gitignore_worktrees(repo_root)?;
        Ok(true)
    } else if issue.description.contains("pre-commit hook not installed") {
        install_pre_commit_hook(repo_root)?;
        Ok(true)
    } else if issue
        .description
        .contains("Claude Code hooks not configured")
    {
        fix_hooks(repo_root)?;
        Ok(true)
    } else if issue.description.contains("Sandbox settings not found") {
        fix_sandbox_settings(repo_root)?;
        Ok(true)
    } else if issue.description.contains("Old unprefixed skill") {
        fix_old_skill(&issue.description)?;
        Ok(true)
    } else if issue.description.contains("Old unprefixed agent") {
        fix_old_agent(&issue.description)?;
        Ok(true)
    } else if issue
        .description
        .contains("Settings.json references old-style skill names")
    {
        fix_settings_skill_refs()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Fix corrupted .work symlink in main repo
fn fix_work_symlink(repo_root: &Path) -> Result<()> {
    let work_path = repo_root.join(".work");
    fs::remove_file(&work_path)
        .with_context(|| format!("Failed to remove .work symlink at {}", work_path.display()))?;
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

/// Install Claude Code hooks and rebuild the skill keyword index
fn fix_hooks(repo_root: &Path) -> Result<()> {
    use crate::fs::permissions::{ensure_loom_permissions, install_loom_hooks};
    install_loom_hooks()?;
    ensure_loom_permissions(repo_root)?;
    rebuild_skill_index()?;
    Ok(())
}

/// Rebuild the skill keyword index by running skill-index-builder.sh
fn rebuild_skill_index() -> Result<()> {
    use crate::fs::permissions::get_installed_hooks_dir;
    let Some(hooks_dir) = get_installed_hooks_dir() else {
        return Ok(());
    };
    let builder: std::path::PathBuf = hooks_dir.join("skill-index-builder.sh");
    if !builder.exists() {
        return Ok(());
    }
    std::process::Command::new(&builder)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .with_context(|| format!("Failed to run {}", builder.display()))?;
    Ok(())
}

/// Apply default sandbox settings
fn fix_sandbox_settings(repo_root: &Path) -> Result<()> {
    use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
    let config = SandboxConfig::default();
    let stage_config = StageSandboxConfig::default();
    let mut merged = sandbox::merge_config(&config, &stage_config, StageType::Standard);
    sandbox::expand_paths(&mut merged);
    sandbox::write_settings(&merged, repo_root)?;
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

/// Remove an old unprefixed skill directory (loom- version already installed).
fn fix_old_skill(description: &str) -> Result<()> {
    let name = description
        .strip_prefix("Old unprefixed skill '")
        .and_then(|s| s.split('\'').next())
        .with_context(|| format!("Cannot parse skill name from: {}", description))?;

    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let old_path = home.join(".claude/skills").join(name);
    fs::remove_dir_all(&old_path)
        .with_context(|| format!("Failed to remove {}", old_path.display()))?;
    Ok(())
}

/// Remove an old unprefixed agent file (loom- version already installed).
fn fix_old_agent(description: &str) -> Result<()> {
    let name = description
        .strip_prefix("Old unprefixed agent '")
        .and_then(|s| s.split('\'').next())
        .with_context(|| format!("Cannot parse agent name from: {}", description))?;

    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let old_path = home.join(".claude/agents").join(format!("{}.md", name));
    fs::remove_file(&old_path)
        .with_context(|| format!("Failed to remove {}", old_path.display()))?;
    Ok(())
}

/// Update old-style skill references in the global settings.json.
///
/// Replaces `Skill({name}` with `Skill(loom-{name}` for each loom-specific
/// skill that does not already have the `loom-` prefix.
fn fix_settings_skill_refs() -> Result<()> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let settings_path = home.join(".claude/settings.json");
    let mut content = fs::read_to_string(&settings_path)
        .with_context(|| format!("Failed to read {}", settings_path.display()))?;

    for name in LOOM_SKILL_NAMES {
        let old_ref = format!("Skill({}", name);
        let new_ref = format!("Skill(loom-{}", name);
        content = content.replace(&old_ref, &new_ref);
    }

    fs::write(&settings_path, &content)
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;
    Ok(())
}
