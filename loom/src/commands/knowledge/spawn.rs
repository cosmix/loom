//! Shared helpers for knowledge commands that spawn Claude sessions.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::work_dir::WorkDir;

/// Resolve the project root directory.
///
/// Tries WorkDir first (works when .work/ exists), then falls back to
/// `git rev-parse --show-toplevel`, then current directory.
pub(super) fn resolve_project_root() -> Result<PathBuf> {
    if let Ok(work_dir) = WorkDir::new(".") {
        if let Some(root) = work_dir.project_root().map(|p| p.to_path_buf()) {
            return Ok(root);
        }
    }

    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git rev-parse")?;

    if output.status.success() {
        let root = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in git output")?
            .trim()
            .to_string();
        return Ok(PathBuf::from(root));
    }

    std::env::current_dir().context("Failed to get current directory")
}

/// Write sandbox settings for a knowledge-scoped Claude session.
///
/// When `allow_writes` is false (dry-run), every file edit is denied.
/// Returns original `.claude/settings.local.json` content for restoration.
///
/// Rules are expressed as `Edit(path)`, never `Write(path)`: Claude Code's file
/// permission check only consults `Edit` rules, so a `Write(**)` deny parses
/// cleanly, prints a warning, and permits every write it was meant to block.
pub(super) fn write_knowledge_sandbox(
    project_root: &Path,
    allow_writes: bool,
) -> Result<Option<String>> {
    let claude_dir = project_root.join(".claude");
    std::fs::create_dir_all(&claude_dir).context("Failed to create .claude directory")?;

    let settings_path = claude_dir.join("settings.local.json");

    let backup = if settings_path.exists() {
        Some(
            std::fs::read_to_string(&settings_path)
                .context("Failed to read existing settings.local.json")?,
        )
    } else {
        None
    };

    let allow = if allow_writes {
        serde_json::json!(["Edit(doc/loom/knowledge/**)", "Bash(loom *)"])
    } else {
        // Dry-run: no write/edit permission anywhere.
        serde_json::json!(["Bash(loom *)"])
    };

    let mut deny = vec![
        "Read(~/.ssh/**)",
        "Read(~/.aws/**)",
        "Read(~/.config/gcloud/**)",
        "Read(~/.gnupg/**)",
    ];
    if !allow_writes {
        // Deny takes precedence over allow, so this blanket rule is only safe in
        // dry-run — in write mode it would also block the knowledge directory.
        deny.push("Edit(**)");
    }

    let settings = serde_json::json!({
        "sandbox": {
            "enabled": true,
            "autoAllowBashIfSandboxed": true,
            "excludedCommands": ["loom"]
        },
        "permissions": {
            "allow": allow,
            "deny": deny
        }
    });

    let content =
        serde_json::to_string_pretty(&settings).context("Failed to serialize sandbox settings")?;
    std::fs::write(&settings_path, content).context("Failed to write sandbox settings")?;

    Ok(backup)
}

/// Restore the caller's settings if loom is interrupted mid-session.
///
/// Ctrl-C reaches the whole foreground process group, so loom dies alongside the
/// Claude session and never reaches its post-spawn restore — leaving the caller's
/// `.claude/settings.local.json` permanently replaced by the sandbox stub.
/// Registering here closes that window; the handler also covers SIGTERM.
pub(super) fn arm_sandbox_restore(project_root: &Path, backup: Option<String>) {
    let root = project_root.to_path_buf();
    let _ = ctrlc::set_handler(move || {
        let _ = restore_sandbox_settings(&root, backup.clone());
        std::process::exit(130);
    });
}

/// Restore original settings after a knowledge session completes.
pub(super) fn restore_sandbox_settings(project_root: &Path, backup: Option<String>) -> Result<()> {
    let settings_path = project_root.join(".claude").join("settings.local.json");
    match backup {
        Some(original) => {
            std::fs::write(&settings_path, original)
                .context("Failed to restore original settings.local.json")?;
        }
        None => {
            let _ = std::fs::remove_file(&settings_path);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests_spawn.rs"]
mod tests;
