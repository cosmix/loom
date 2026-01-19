//! Hooks command implementation
//!
//! Provides commands for managing loom hooks independently of full orchestration.
//! Useful for developers who want to use loom hooks without running a full plan.

use anyhow::{Context, Result};
use std::env;

use crate::fs::permissions::{ensure_loom_permissions, install_loom_hooks};

/// Install loom hooks to the current project
///
/// This command:
/// 1. Installs hook scripts to ~/.claude/hooks/loom/
/// 2. Configures .claude/settings.json with hooks and permissions
///
/// After running this command, hooks will be active for all Claude Code
/// sessions in this project without needing to run `loom init` with a plan.
pub fn install() -> Result<()> {
    println!("Installing loom hooks...\n");

    // Find repository root (where .git is)
    let repo_root = find_repo_root().context("Not in a git repository")?;

    // Install hooks to ~/.claude/hooks/loom/
    let scripts_installed = install_loom_hooks()?;
    if scripts_installed > 0 {
        println!("  Installed {scripts_installed} hook script(s) to ~/.claude/hooks/loom/");
    } else {
        println!("  Hook scripts already up to date in ~/.claude/hooks/loom/");
    }

    // Configure .claude/settings.json with hooks and permissions
    ensure_loom_permissions(&repo_root)?;

    println!("\nHooks installed successfully!");
    println!("\nActive hooks:");
    println!("  PreToolUse:");
    println!("    - prefer-modern-tools.sh  Suggests Grep/Glob or fd/rg instead of grep/find");
    println!("    - commit-filter.sh        Blocks forbidden patterns in git commits");
    println!("    - ask-user-pre.sh         Marks stage as waiting for user input");
    println!("  PostToolUse:");
    println!("    - ask-user-post.sh        Resumes stage after user input");
    println!("  Stop:");
    println!("    - commit-guard.sh         Enforces commit before session end (in worktrees)");
    println!("  UserPromptSubmit:");
    println!("    - skill-trigger.sh        Suggests skills based on prompt keywords");
    println!();
    println!("Hooks are now active for all Claude Code sessions in this project.");

    Ok(())
}

/// List available loom hooks and their status
pub fn list() -> Result<()> {
    // Find repository root
    let repo_root = find_repo_root().context("Not in a git repository")?;
    let settings_path = repo_root.join(".claude/settings.json");

    // Check if hooks are configured in this project
    let project_hooks = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read {}", settings_path.display()))?;
        let settings: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", settings_path.display()))?;
        settings.get("hooks").cloned()
    } else {
        None
    };

    // Check if hook scripts are installed globally
    let home_dir = dirs::home_dir();
    let scripts_installed = home_dir
        .as_ref()
        .map(|h| h.join(".claude/hooks/loom").exists())
        .unwrap_or(false);

    println!("Loom hooks status:\n");

    // Project status
    if let Some(hooks) = &project_hooks {
        println!("Project: CONFIGURED");
        println!("  Settings: {}", settings_path.display());
        println!();

        // Show configured hooks
        if let Some(obj) = hooks.as_object() {
            for (event, rules) in obj {
                println!("{event}:");
                if let Some(rules_arr) = rules.as_array() {
                    for rule in rules_arr {
                        if let (Some(matcher), Some(hooks_inner)) =
                            (rule.get("matcher"), rule.get("hooks"))
                        {
                            let matcher_str = matcher.as_str().unwrap_or("*");
                            if let Some(hooks_arr) = hooks_inner.as_array() {
                                for hook in hooks_arr {
                                    if let Some(cmd) = hook.get("command").and_then(|c| c.as_str())
                                    {
                                        let script_name = std::path::Path::new(cmd)
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or(cmd);
                                        println!("  [{matcher_str}] -> {script_name}");
                                    }
                                }
                            }
                        }
                    }
                }
                println!();
            }
        }
    } else {
        println!("Project: NOT CONFIGURED");
        if settings_path.exists() {
            println!("  Settings file exists but has no hooks section");
        } else {
            println!("  No .claude/settings.json found");
        }
        println!();
        println!("Run 'loom hooks install' to configure hooks for this project.");
        println!();
    }

    // Global scripts status
    if scripts_installed {
        if let Some(home) = home_dir {
            println!(
                "Hook scripts: INSTALLED at {}",
                home.join(".claude/hooks/loom").display()
            );
        }
    } else {
        println!("Hook scripts: NOT INSTALLED");
        println!("  Run 'loom hooks install' to install hook scripts.");
    }

    // Show available hooks if not configured
    if project_hooks.is_none() {
        println!();
        println!("Available loom hooks:");
        println!("  PreToolUse:");
        println!("    - prefer-modern-tools.sh  Suggests Grep/Glob or fd/rg instead of grep/find");
        println!("    - commit-filter.sh        Blocks forbidden patterns in git commits");
        println!("    - ask-user-pre.sh         Marks stage as waiting for user input");
        println!("  PostToolUse:");
        println!("    - ask-user-post.sh        Resumes stage after user input");
        println!("  Stop:");
        println!("    - commit-guard.sh         Enforces commit before session end");
        println!("  UserPromptSubmit:");
        println!("    - skill-trigger.sh        Suggests skills based on prompt keywords");
    }

    Ok(())
}

/// Find the repository root directory (containing .git)
fn find_repo_root() -> Result<std::path::PathBuf> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let mut dir = current_dir.as_path();

    loop {
        if dir.join(".git").exists() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => anyhow::bail!("Not in a git repository"),
        }
    }
}
