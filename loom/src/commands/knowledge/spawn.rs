//! Shared helpers for knowledge commands that spawn Claude sessions.

use anyhow::{anyhow, bail, Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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

/// Exception-safe, process-locked replacement for knowledge-session settings.
pub(super) struct KnowledgeSandboxGuard {
    settings_path: PathBuf,
    backup: Option<String>,
    installed_content: String,
    active: Arc<AtomicBool>,
    _lock: File,
}

impl KnowledgeSandboxGuard {
    /// Install temporary settings while holding a stable-inode exclusive lock.
    pub(super) fn install(project_root: &Path, allow_writes: bool) -> Result<Self> {
        let claude_dir = project_root.join(".claude");
        std::fs::create_dir_all(&claude_dir).context("Failed to create .claude directory")?;

        let lock_path = claude_dir.join(".loom-knowledge-settings.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| {
                format!(
                    "Failed to open knowledge settings lock: {}",
                    lock_path.display()
                )
            })?;
        lock.try_lock_exclusive().with_context(|| {
            format!(
                "Another loom knowledge session already owns {}",
                lock_path.display()
            )
        })?;

        let settings_path = claude_dir.join("settings.local.json");
        let backup = if settings_path.exists() {
            Some(
                std::fs::read_to_string(&settings_path)
                    .context("Failed to read existing settings.local.json")?,
            )
        } else {
            None
        };
        let installed_content = knowledge_sandbox_content(allow_writes)?;
        crate::fs::locking::locked_write(&settings_path, &installed_content)
            .context("Failed to write sandbox settings")?;

        Ok(Self {
            settings_path,
            backup,
            installed_content,
            active: Arc::new(AtomicBool::new(true)),
            _lock: lock,
        })
    }

    /// Restore the exact prior file and surface any restoration failure.
    pub(super) fn restore(&mut self) -> Result<()> {
        if !self.active.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        restore_snapshot(
            &self.settings_path,
            self.backup.as_deref(),
            &self.installed_content,
        )
    }
}

impl Drop for KnowledgeSandboxGuard {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("Failed to restore knowledge sandbox settings: {error:#}");
        }
    }
}

/// Always attempt restoration and preserve both errors when execution and
/// cleanup fail together.
pub(super) fn restore_after<T>(
    guard: &mut KnowledgeSandboxGuard,
    operation: Result<T>,
) -> Result<T> {
    let restoration = guard.restore();
    match (operation, restoration) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(operation_error), Err(restoration_error)) => Err(anyhow!(
            "Knowledge session failed: {operation_error:#}; settings restoration also failed: {restoration_error:#}"
        )),
    }
}

/// Build settings for a knowledge-scoped session.
///
/// Rules are expressed as `Edit(path)`, never `Write(path)`: the file
/// permission check only consults `Edit` rules.
fn knowledge_sandbox_content(allow_writes: bool) -> Result<String> {
    let allow = if allow_writes {
        serde_json::json!(["Edit(doc/loom/knowledge/**)", "Bash(loom *)"])
    } else {
        // Dry-run: no write/edit permission anywhere.
        serde_json::json!(["Bash(loom *)"])
    };

    let secret_paths = [
        "~/.ssh/**",
        "~/.aws/**",
        "~/.config/gcloud/**",
        "~/.gnupg/**",
        "~/.claude/.credentials.json",
        "~/.claude.json",
    ];
    let mut deny: Vec<String> = secret_paths
        .iter()
        .map(|path| format!("Read({path})"))
        .collect();
    if !allow_writes {
        // Deny takes precedence over allow, so this blanket rule is only safe in
        // dry-run — in write mode it would also block the knowledge directory.
        deny.push("Edit(**)".to_string());
    }

    let settings = serde_json::json!({
        "sandbox": {
            "enabled": true,
            "filesystem": {
                "denyRead": secret_paths
            }
        },
        "permissions": {
            "allow": allow,
            "deny": deny
        }
    });

    serde_json::to_string_pretty(&settings).context("Failed to serialize sandbox settings")
}

fn restore_snapshot(settings_path: &Path, backup: Option<&str>, expected: &str) -> Result<()> {
    let current = std::fs::read_to_string(settings_path)
        .context("Failed to read temporary settings before restoration")?;
    if current != expected {
        bail!(
            "Refusing to overwrite settings.local.json because it changed during the knowledge session"
        );
    }

    match backup {
        Some(original) => crate::fs::locking::locked_write(settings_path, original)
            .context("Failed to restore original settings.local.json"),
        None => {
            let parent = settings_path
                .parent()
                .context("Knowledge settings path has no parent")?;
            crate::fs::locking::locked_dir_update(parent, || {
                std::fs::remove_file(settings_path)
                    .context("Failed to remove temporary settings.local.json")
            })
        }
    }
}

/// Restore the caller's settings if the foreground process receives Ctrl-C.
pub(super) fn arm_sandbox_restore(guard: &KnowledgeSandboxGuard) -> Result<()> {
    let settings_path = guard.settings_path.clone();
    let backup = guard.backup.clone();
    let expected = guard.installed_content.clone();
    let active = Arc::clone(&guard.active);
    ctrlc::set_handler(move || {
        if active.swap(false, Ordering::AcqRel) {
            if let Err(error) = restore_snapshot(&settings_path, backup.as_deref(), &expected) {
                eprintln!(
                    "Failed to restore knowledge sandbox settings after interrupt: {error:#}"
                );
                std::process::exit(1);
            }
        }
        std::process::exit(130);
    })
    .context("Failed to install knowledge sandbox interrupt handler")
}

#[cfg(test)]
#[path = "tests_spawn.rs"]
mod tests;
