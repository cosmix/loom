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
use std::process::Command;

use crate::daemon::{DaemonServer, DaemonStatus};
use crate::fs::permissions::LOOM_PERMISSIONS;
use crate::fs::work_dir::load_config;
use crate::fs::work_integrity::{
    check_work_dir_state, is_work_dir_git_ignored, is_worktrees_git_ignored, WorkDirState,
};
use crate::git::branch::{is_ancestor_of, resolve_target_branch};
use crate::git::{
    branch_exists, branch_name_for_stage, install_pre_commit_hook, is_pre_commit_hook_installed,
};
use crate::models::stage::StageType;
use crate::sandbox;
use crate::verify::transitions::{list_all_stages, update_stage};

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

    // Check 5: .claude/settings.json has permissions (but NOT hooks/env - those belong in settings.local.json)
    {
        let settings_path = repo_root.join(".claude/settings.json");
        let parsed = if settings_path.exists() {
            std::fs::read_to_string(&settings_path)
                .ok()
                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        } else {
            None
        };

        let mut missing_reasons: Vec<&str> = Vec::new();

        if let Some(ref val) = parsed {
            // Check permissions.allow contains all LOOM_PERMISSIONS entries
            let has_all_perms = val
                .get("permissions")
                .and_then(|p| p.get("allow"))
                .and_then(|a| a.as_array())
                .map(|arr| {
                    let allowed: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                    LOOM_PERMISSIONS.iter().all(|perm| allowed.contains(perm))
                })
                .unwrap_or(false);
            if !has_all_perms {
                missing_reasons.push("permissions missing");
            }
        } else {
            // File missing or unparseable
            missing_reasons.push("file missing");
        }

        if !missing_reasons.is_empty() {
            let reasons = missing_reasons.join(", ");
            issues.push(RepairIssue {
                severity: Severity::Info,
                description: format!("Project .claude/settings.json incomplete ({})", reasons),
                fix_description: "Restore permissions to .claude/settings.json".to_string(),
            });
        }
    }

    // Checks 5b and 6: the .claude settings files and ~/.codex/config.toml
    issues.extend(settings_checks::check(repo_root));

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

    // Check 11: Phantom merge audit — stages marked merged without their commit in the target branch.
    //
    // WHY THIS EXISTS: A bug in the orchestrator's "defensive fallback" paths can write
    // `merged = true` on a stage whose branch was never actually git-merged into the target
    // branch. This silently gates dependent stages on work that never landed, causing lost work.
    // This check provides a post-hoc safety net that users can run (or that CI can run) to
    // detect these phantom merges before they cause further damage.
    //
    // Only runs when .work/stages/ exists. Skips Knowledge stages (they legitimately have
    // `merged = true` with no branch/commit — that's by design). For all other stages:
    //   - merged=true + commit present -> verify commit is an ancestor of target branch
    //   - merged=true + no commit      -> warn (cannot verify, needs manual check)
    //   - Completed + !merged + branch gone -> warn (branch deleted without merge confirmation)
    {
        let work_dir = repo_root.join(".work");
        if work_dir.is_dir() {
            // Determine the target branch for ancestry checks.
            // Load from config.toml if available, otherwise fall back to repo default.
            let base_branch_opt = load_config(&work_dir)
                .ok()
                .flatten()
                .and_then(|c| c.base_branch());
            let target_branch = resolve_target_branch(&base_branch_opt, repo_root);

            match list_all_stages(&work_dir) {
                Err(_) => {
                    // Cannot enumerate stages (e.g., stages dir missing or unparseable).
                    // Push an INFO rather than failing the whole repair run.
                    issues.push(RepairIssue {
                        severity: Severity::Info,
                        description:
                            "Could not audit stage merge status (stages directory unreadable)"
                                .to_string(),
                        fix_description: "Investigate .work/stages/ directory manually".to_string(),
                    });
                }
                Ok(stages) => {
                    for stage in &stages {
                        // Knowledge stages legitimately have merged=true with no commit —
                        // they have no branch and no git work to verify. Skip them.
                        if stage.stage_type == StageType::Knowledge {
                            continue;
                        }

                        if stage.merged {
                            if let Some(ref commit) = stage.completed_commit {
                                // CRITICAL: merged=true and we have a commit SHA.
                                // Verify the commit is actually an ancestor of the target branch.
                                // If it isn't, the stage was marked merged without a real merge.
                                match is_ancestor_of(commit, &target_branch, repo_root) {
                                    Ok(true) => {
                                        // All good — commit is in target branch.
                                    }
                                    Ok(false) => {
                                        // Phantom merge: commit exists but is not in target branch.
                                        issues.push(RepairIssue {
                                            severity: Severity::Critical,
                                            description: format!(
                                                "Phantom merge: {} marked merged but commit not in {}",
                                                stage.id, target_branch
                                            ),
                                            fix_description:
                                                "Revert merged flag to false (manual investigation needed for lost work)"
                                                    .to_string(),
                                        });
                                    }
                                    Err(_) => {
                                        // Git is unavailable or the commit/branch reference is
                                        // broken. Skip silently rather than producing noise.
                                    }
                                }
                            } else {
                                // WARNING: merged=true but no commit SHA to verify against.
                                // Cannot confirm whether the work actually landed.
                                issues.push(RepairIssue {
                                    severity: Severity::Warning,
                                    description: format!(
                                        "Stage {} marked merged but has no completed_commit (cannot verify)",
                                        stage.id
                                    ),
                                    fix_description:
                                        "No automatic fix available — manual investigation required"
                                            .to_string(),
                                });
                            }
                        } else if stage.status == crate::models::stage::StageStatus::Completed {
                            // WARNING: stage is Completed but not merged, and the branch is gone.
                            // This suggests the branch was deleted without a merge being recorded.
                            // The work might have been merged manually without updating loom state.
                            let branch = branch_name_for_stage(&stage.id);
                            match branch_exists(&branch, repo_root) {
                                Ok(false) => {
                                    issues.push(RepairIssue {
                                        severity: Severity::Warning,
                                        description: format!(
                                            "Stale: {} completed but branch deleted without merge confirmation",
                                            stage.id
                                        ),
                                        fix_description:
                                            "No automatic fix available — verify the work was merged manually"
                                                .to_string(),
                                    });
                                }
                                Ok(true) | Err(_) => {
                                    // Branch still exists (normal unmerged state) or git is
                                    // unavailable. Nothing to flag here.
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Check 12: Daemon health — detect the singleton/socket failure modes
    // documented in concerns.md (2026-05-13). These are diagnostic-only: there is
    // no safe automatic fix (killing the wrong daemon loses orchestration state),
    // so each is reported with manual remediation guidance.
    {
        let work_dir = repo_root.join(".work");
        if work_dir.is_dir() {
            // (1) More than one `loom run` process alive is always wrong.
            let run_pids = find_loom_run_pids();
            if run_pids.len() > 1 {
                let pid_list = run_pids
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let lock_pid = DaemonServer::check_lock(&work_dir);
                let keep_hint = match lock_pid {
                    Some(pid) => format!(
                        "Keep the lock holder (PID {pid}); stop the others with `kill <pid>`"
                    ),
                    None => "Stop the stale daemons with `kill <pid>` (no lock holder found)"
                        .to_string(),
                };
                issues.push(RepairIssue {
                    severity: Severity::Critical,
                    description: format!("Multiple 'loom run' processes alive (PIDs: {pid_list})"),
                    fix_description: keep_hint,
                });
            }

            // (2)/(3) Lock held by a live daemon, but PID file or socket missing.
            if let Some(lock_pid) = DaemonServer::check_lock(&work_dir) {
                if crate::process::is_process_alive(lock_pid) {
                    let pid_path = work_dir.join("orchestrator.pid");
                    let socket_path = work_dir.join("orchestrator.sock");

                    if !pid_path.exists() {
                        issues.push(RepairIssue {
                            severity: Severity::Warning,
                            description: format!(
                                "Daemon lock held (PID {lock_pid}) but orchestrator.pid is missing"
                            ),
                            fix_description:
                                "Restart the daemon: `loom stop`, then `loom run` (PID file was lost)"
                                    .to_string(),
                        });
                    }

                    if !socket_path.exists() {
                        issues.push(RepairIssue {
                            severity: Severity::Critical,
                            description: format!(
                                "Daemon lock held (PID {lock_pid}) but orchestrator.sock is missing (daemon unreachable)"
                            ),
                            fix_description:
                                "Restart the daemon: `kill <pid>` then `loom run` (control socket was lost)"
                                    .to_string(),
                        });
                    }
                }
            } else if DaemonServer::check_status(&work_dir) == DaemonStatus::ProcessOnly {
                // No flock holder, but a daemon process appears alive with an
                // unreachable socket (legacy daemon started before the flock fix).
                issues.push(RepairIssue {
                    severity: Severity::Warning,
                    description: "Daemon process appears alive but its socket is unreachable"
                        .to_string(),
                    fix_description: "Restart the daemon: `loom stop`, then `loom run`".to_string(),
                });
            }
        }
    }

    // Check 13: stale doc/loom/knowledge deny in .claude/settings.local.json.
    issues.extend(check_stale_knowledge_denies(repo_root));

    issues
}

/// Check 13: stale doc/loom/knowledge deny in .claude/settings.local.json.
///
/// A settings file written before the knowledge-directory sandbox grant
/// existed can carry `Edit(doc/loom/knowledge/**)` / `Write(doc/loom/knowledge/**)`
/// in `permissions.deny` alongside the (shadowed, harmless) `allowWrite`
/// grant — deny wins, so the `loom knowledge update` CLI subprocess stays
/// blocked for that checkout until the file is regenerated.
/// `write_settings`'s scrub (`merge_existing_permissions`) heals this
/// automatically on regeneration, but nothing today prompts for one; this
/// check is that prompt. Checked in both the main repo and every worktree,
/// since each has its own settings.local.json.
fn check_stale_knowledge_denies(repo_root: &Path) -> Vec<RepairIssue> {
    existing_settings_local_files(repo_root)
        .into_iter()
        .filter(|settings_path| settings_local_has_stale_knowledge_deny(settings_path))
        .map(|settings_path| RepairIssue {
            severity: Severity::Warning,
            description: format!(
                "Stale knowledge-directory deny in {}",
                settings_path.display()
            ),
            fix_description: "Regenerate sandbox settings so the knowledge grant is not \
                               shadowed by a stale deny"
                .to_string(),
        })
        .collect()
}

/// Every `.claude/settings.local.json` that currently exists: the main
/// repo's, plus one per worktree that already has its own. A worktree
/// without a settings file yet gets one when its own stage session starts,
/// so it is not a repair issue and is skipped here.
fn existing_settings_local_files(repo_root: &Path) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();

    let main_settings = repo_root.join(".claude/settings.local.json");
    if main_settings.exists() {
        paths.push(main_settings);
    }

    if let Ok(entries) = fs::read_dir(repo_root.join(".worktrees")) {
        for entry in entries.flatten() {
            let settings_path = entry.path().join(".claude/settings.local.json");
            if entry.path().is_dir() && settings_path.exists() {
                paths.push(settings_path);
            }
        }
    }

    paths
}

/// Whether a `permissions.deny` entry names the knowledge directory, in
/// either the enforced `Edit(...)` form or the inert-but-OS-leaking
/// `Write(...)` form (see `sandbox::settings::merge_existing_permissions`'s
/// doc comment for why both matter). Shared by the detector
/// (`settings_local_has_stale_knowledge_deny`) and the worktree scalpel
/// (`strip_stale_knowledge_denies`) so the two can never drift apart on what
/// counts as "stale".
fn is_knowledge_dir_deny_entry(entry: &str) -> bool {
    entry.starts_with("Edit(doc/loom/knowledge") || entry.starts_with("Write(doc/loom/knowledge")
}

/// Whether a `.claude/settings.local.json` file at `path` carries a stale
/// `permissions.deny` entry for the knowledge directory. Any parse failure
/// is treated as "no issue" — this is a diagnostic nudge, not a validator,
/// and `loom repair` already has other checks for a malformed settings file.
fn settings_local_has_stale_knowledge_deny(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    value["permissions"]["deny"]
        .as_array()
        .map(|deny| {
            deny.iter()
                .any(|entry| entry.as_str().is_some_and(is_knowledge_dir_deny_entry))
        })
        .unwrap_or(false)
}

/// Enumerate the PIDs of currently-running `loom run` processes.
///
/// Uses `ps aux` (portable across Linux and macOS, matching the existing
/// process-scan pattern in `native/pid_tracking.rs`) and matches command lines
/// containing the `loom run` invocation, excluding this `loom repair` process.
/// On any `ps` failure returns an empty vec — the daemon-health checks degrade to
/// "no duplicates detected" rather than failing the whole repair run.
fn find_loom_run_pids() -> Vec<u32> {
    let our_pid = std::process::id();
    let output = match Command::new("ps")
        .arg("axww")
        .arg("-o")
        .arg("pid=,args=")
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut pids = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_start();
        let mut parts = line.splitn(2, char::is_whitespace);
        let pid: u32 = match parts.next().and_then(|p| p.trim().parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        let args = parts.next().unwrap_or("");
        if pid == our_pid {
            continue;
        }
        // Match the `loom run` invocation. Require the program component to end in
        // `loom` and the next token to be `run` so unrelated commands that merely
        // mention the words (e.g. an editor on this file) are not counted.
        if is_loom_run_cmdline(args) {
            pids.push(pid);
        }
    }
    pids
}

/// Return true if `args` is a `loom run ...` command line.
fn is_loom_run_cmdline(args: &str) -> bool {
    let mut tokens = args.split_whitespace();
    let Some(program) = tokens.next() else {
        return false;
    };
    // The program token may be a path like `/usr/local/bin/loom` or `loom`.
    let prog_name = program.rsplit('/').next().unwrap_or(program);
    if prog_name != "loom" {
        return false;
    }
    tokens.next() == Some("run")
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
        .contains("Project .claude/settings.json incomplete")
        || issue
            .description
            .contains("Hooks found in .claude/settings.json")
    {
        fix_hooks(repo_root)?;
        Ok(true)
    } else if issue
        .description
        .contains("Settings not found (.claude/settings.local.json)")
    {
        fix_sandbox_settings(repo_root)?;
        fix_hooks_local(repo_root)?;
        Ok(true)
    } else if issue
        .description
        .contains("Stale knowledge-directory deny in")
    {
        // Must be matched before the generic ".claude/settings.local.json"
        // arm below, which would otherwise swallow this issue (its
        // description names that file too) and skip the sandbox
        // regeneration that actually scrubs the stale deny.
        fix_sandbox_settings(repo_root)?;
        fix_hooks_local(repo_root)?;
        Ok(true)
    } else if issue.description.contains(".claude/settings.local.json") {
        // Everything else that names this file — missing hooks/env, missing
        // codex sandbox allowances — is repaired by rewriting it. The
        // file-absent case is claimed by the arm above, which runs first.
        fix_hooks_local(repo_root)?;
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
    } else if issue.description.contains("exclude_slash_tmp") {
        settings_checks::fix_codex_slash_tmp()
    } else if issue.description.starts_with("Phantom merge:") {
        // Revert the spurious merged=true flag so the orchestrator knows the stage's work
        // has NOT landed in the target branch. We do NOT attempt a re-merge here because
        // the user likely has lost work that needs manual investigation first (e.g.,
        // cherry-pick from the stranded branch, resolve conflicts with later stages).
        fix_phantom_merge(repo_root, &issue.description)?;
        Ok(true)
    } else {
        // Everything else — "marked merged but has no completed_commit" (no SHA
        // to verify or re-merge against), "Stale:" (branch gone without a merge
        // record), and any unknown issue — returns false so the dispatcher
        // prints "Skipped" and the user knows to investigate manually.
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
    // The settings files may carry a LOOM_WORK_DIR pin naming the directory
    // we just deleted. Left in place, it shadows WorkDir::new's upward
    // search in every later session of this repo (see
    // scrub_stale_work_dir_env), so heal it now rather than leaving the very
    // next session to resolve a dead path.
    crate::fs::permissions::scrub_main_repo_settings_identity(repo_root);
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

/// Install Claude Code hooks, configure permissions, and rebuild the skill keyword index
fn fix_hooks(repo_root: &Path) -> Result<()> {
    use crate::fs::permissions::{ensure_loom_permissions, install_loom_hooks};
    fix_hooks_with(
        repo_root,
        || install_loom_hooks().map(|_| ()),
        ensure_loom_permissions,
        rebuild_skill_index,
    )?;
    Ok(())
}

fn fix_hooks_with<I, P, R>(repo_root: &Path, install: I, permissions: P, rebuild: R) -> Result<()>
where
    I: FnOnce() -> Result<()>,
    P: FnOnce(&Path) -> Result<()>,
    R: FnOnce() -> Result<()>,
{
    install()?;
    permissions(repo_root)?;
    rebuild()
}

/// Configure hooks and env in settings.local.json
fn fix_hooks_local(repo_root: &Path) -> Result<()> {
    use crate::fs::permissions::ensure_loom_hooks_local;
    ensure_loom_hooks_local(repo_root)?;
    Ok(())
}

/// Rebuild the skill keyword index using the built-in skill_index command
fn rebuild_skill_index() -> Result<()> {
    super::skill_index::execute()
}

/// Apply default sandbox settings to `target`'s `.claude/settings.local.json`.
///
/// `target` may be the main repo root or a worktree root — `write_settings`
/// resolves `target_is_worktree` itself from the path it is given, so a
/// fresh `merge_config` per target (rather than reusing one merged config
/// across targets) keeps that resolution correct for whichever `target` this
/// call was given.
///
/// Always passes `&Implementers::default()` (claude-only), matching the
/// existing main-repo behavior: this repairs the DEFAULT sandbox, not
/// whatever lane a stage's own plan may have licensed. See the doc comment
/// on `sandbox::settings::preserve_unowned_keys` for why the claude-only
/// default here is deliberate — restoring an actual codex license is a
/// separate concern from repairing a broken default.
fn write_default_sandbox_settings(target: &Path) -> Result<()> {
    use crate::models::stage::Implementers;
    use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
    let mut merged = sandbox::merge_config(
        &SandboxConfig::default(),
        &StageSandboxConfig::default(),
        StageType::Standard,
        &Implementers::default(),
    );
    sandbox::expand_paths(&mut merged);
    sandbox::write_settings(&merged, target)?;
    Ok(())
}

/// Remove stale doc/loom/knowledge `permissions.deny` entries from a
/// worktree's `.claude/settings.local.json`, in place — every other key
/// (the rest of `permissions`, the `sandbox` block, plugin keys, anything
/// else) is left exactly as it was.
///
/// Unlike the main repo (regenerated wholesale by
/// `write_default_sandbox_settings`), a worktree's settings file is the
/// sandbox of a possibly LIVE stage session, and it legitimately differs
/// from the default: a codex-licensed stage carries `~/.codex` write grants
/// and the codex domains, and any stage carries its plan's own
/// `allow_write` entries. Regenerating it from `SandboxConfig::default()`
/// would silently narrow a running stage's sandbox mid-session — a bigger
/// hazard than the stale deny being healed, and one the stage would not
/// recover from until its next respawn. So only the offending entries are
/// stripped.
fn strip_stale_knowledge_denies(path: &Path) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    if let Some(deny) = value
        .pointer_mut("/permissions/deny")
        .and_then(|d| d.as_array_mut())
    {
        deny.retain(|entry| !entry.as_str().is_some_and(is_knowledge_dir_deny_entry));
    }

    let updated = serde_json::to_string_pretty(&value)
        .with_context(|| format!("Failed to serialize {}", path.display()))?;
    fs::write(path, updated).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Apply default sandbox settings to the main repo, and scalpel the stale
/// doc/loom/knowledge deny out of every worktree that carries one.
///
/// The main repo's settings file is loom's own default sandbox (used for
/// knowledge-type stages and general repair), so full regeneration is
/// correct there — see `write_default_sandbox_settings`. A worktree's
/// settings file is not: see `strip_stale_knowledge_denies` for why it gets
/// a targeted fix instead of regeneration. A worktree with no stale deny,
/// or no settings file yet, is left untouched.
fn fix_sandbox_settings(repo_root: &Path) -> Result<()> {
    write_default_sandbox_settings(repo_root)?;

    if let Ok(entries) = fs::read_dir(repo_root.join(".worktrees")) {
        for entry in entries.flatten() {
            let settings_path = entry.path().join(".claude/settings.local.json");
            if entry.path().is_dir() && settings_local_has_stale_knowledge_deny(&settings_path) {
                strip_stale_knowledge_denies(&settings_path)?;
            }
        }
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

/// Revert the `merged` flag on a phantom-merged stage.
///
/// A phantom merge is a stage that has `merged = true` in its state file but whose
/// `completed_commit` is not an ancestor of the target branch — meaning the branch
/// was never actually git-merged. This function sets `merged = false` so the
/// orchestrator treats the stage as unmerged and does not let dependents proceed on
/// the assumption that the work landed.
///
/// We deliberately do NOT attempt a re-merge here. The user's repository may be in
/// an inconsistent state (conflicting later stages, stranded commits) that requires
/// manual investigation before another merge is safe.
fn fix_phantom_merge(repo_root: &Path, description: &str) -> Result<()> {
    // Parse stage ID from description: "Phantom merge: <stage-id> marked merged but ..."
    let stage_id = description
        .strip_prefix("Phantom merge: ")
        .and_then(|s| s.split(' ').next())
        .with_context(|| format!("Cannot parse stage ID from: {description}"))?;

    let work_dir = repo_root.join(".work");
    update_stage(stage_id, &work_dir, |stage| {
        stage.merged = false;
        Ok(())
    })
    .with_context(|| format!("Failed to save stage '{stage_id}' after reverting merged flag"))?;

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

mod settings_checks;

#[cfg(test)]
mod tests;
