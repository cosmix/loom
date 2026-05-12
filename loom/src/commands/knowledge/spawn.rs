//! Shared helpers for knowledge commands that spawn Claude sessions.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::knowledge::KnowledgeDir;
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

/// Read existing knowledge files and format them for context embedding.
///
/// Files that only contain the default template (≤5 lines) are skipped.
pub(super) fn read_existing_knowledge(knowledge: &KnowledgeDir) -> String {
    if !knowledge.exists() {
        return String::new();
    }

    let mut sections = Vec::new();
    if let Ok(files) = knowledge.read_all() {
        for (file_type, content) in files {
            let trimmed = content.trim().to_string();
            if trimmed.lines().count() > 5 {
                sections.push(format!(
                    "### Existing {}\n\n{}",
                    file_type.filename(),
                    trimmed
                ));
            }
        }
    }

    if sections.is_empty() {
        return String::new();
    }

    format!(
        "## Existing Knowledge (DO NOT DUPLICATE)\n\n\
         The following knowledge has already been documented. \
         Do NOT repeat this information. Only add NEW discoveries.\n\n{}",
        sections.join("\n\n---\n\n")
    )
}

/// Write sandbox settings for a knowledge-scoped Claude session.
///
/// When `allow_writes` is false (dry-run), Write is denied entirely.
/// Returns original `.claude/settings.local.json` content for restoration.
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
        serde_json::json!([
            "Write(doc/loom/knowledge/**)",
            "Edit(doc/loom/knowledge/**)",
            "Bash(loom *)"
        ])
    } else {
        // Dry-run: no write/edit permission anywhere.
        serde_json::json!(["Bash(loom *)"])
    };

    let settings = serde_json::json!({
        "sandbox": {
            "enabled": true,
            "autoAllowBashIfSandboxed": true,
            "excludedCommands": ["loom"]
        },
        "permissions": {
            "allow": allow,
            "deny": [
                "Read(~/.ssh/**)",
                "Read(~/.aws/**)",
                "Read(~/.config/gcloud/**)",
                "Read(~/.gnupg/**)",
                "Write(**)"
            ]
        }
    });

    let content =
        serde_json::to_string_pretty(&settings).context("Failed to serialize sandbox settings")?;
    std::fs::write(&settings_path, content).context("Failed to write sandbox settings")?;

    Ok(backup)
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
