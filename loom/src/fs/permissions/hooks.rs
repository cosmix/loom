//! Hook configuration and installation for loom

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::constants::LOOM_STOP_HOOK;

/// Remove legacy flux-stop.sh hook and its references from settings
///
/// This cleans up the old "flux" naming convention that was replaced by "loom".
/// It removes:
/// - ~/.claude/hooks/flux-stop.sh file
/// - Any flux-stop.sh references in ~/.claude/settings.json
///
/// # Returns
/// - `Ok(true)` if any legacy artifacts were removed
/// - `Ok(false)` if no legacy artifacts were found
/// - `Err` if removal failed
pub fn remove_legacy_hooks() -> Result<bool> {
    let home_dir = dirs::home_dir().context("Failed to determine home directory")?;
    let mut removed_anything = false;

    // Remove legacy flux-stop.sh hook file
    // Use try_exists() to handle broken symlinks, and ignore NotFound errors
    let legacy_hook_path = home_dir.join(".claude/hooks/flux-stop.sh");
    match fs::remove_file(&legacy_hook_path) {
        Ok(()) => removed_anything = true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File doesn't exist, nothing to remove
        }
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Failed to remove legacy hook at {}",
                    legacy_hook_path.display()
                )
            });
        }
    }

    // Remove flux-stop.sh references from ~/.claude/settings.json
    let settings_path = home_dir.join(".claude/settings.json");
    if settings_path.exists() {
        let removed_from_settings = remove_legacy_hooks_from_settings(&settings_path)?;
        removed_anything = removed_anything || removed_from_settings;
    }

    Ok(removed_anything)
}

/// Remove flux-stop.sh references from a settings file
fn remove_legacy_hooks_from_settings(settings_path: &Path) -> Result<bool> {
    let content = fs::read_to_string(settings_path)
        .with_context(|| format!("Failed to read {}", settings_path.display()))?;

    let mut settings: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {} as JSON", settings_path.display()))?;

    let mut modified = false;

    if let Some(settings_obj) = settings.as_object_mut() {
        if let Some(hooks) = settings_obj.get_mut("hooks") {
            if let Some(hooks_obj) = hooks.as_object_mut() {
                // Check Stop hooks for flux-stop.sh references
                if let Some(stop_hooks) = hooks_obj.get_mut("Stop") {
                    if let Some(stop_arr) = stop_hooks.as_array_mut() {
                        let original_len = stop_arr.len();

                        // Filter out any hook entries containing flux-stop.sh
                        stop_arr.retain(|hook_entry| {
                            if let Some(hooks) = hook_entry.get("hooks").and_then(|h| h.as_array())
                            {
                                !hooks.iter().any(|hook| {
                                    hook.get("command")
                                        .and_then(|c| c.as_str())
                                        .is_some_and(|cmd| cmd.contains("flux-stop.sh"))
                                })
                            } else {
                                true // Keep entries without hooks array
                            }
                        });

                        if stop_arr.len() != original_len {
                            modified = true;
                        }
                    }
                }
            }
        }
    }

    if modified {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;

        fs::write(settings_path, content)
            .with_context(|| format!("Failed to write {}", settings_path.display()))?;
    }

    Ok(modified)
}

/// Generate hooks configuration for loom
/// Hooks reference scripts at ~/.claude/hooks/ (installed by loom init)
pub fn loom_hooks_config() -> Value {
    json!({
        "PreToolUse": [
            {
                "matcher": "AskUserQuestion",
                "hooks": [
                    {
                        "type": "command",
                        "command": "~/.claude/hooks/ask-user-pre.sh"
                    }
                ]
            }
        ],
        "PostToolUse": [
            {
                "matcher": "AskUserQuestion",
                "hooks": [
                    {
                        "type": "command",
                        "command": "~/.claude/hooks/ask-user-post.sh"
                    }
                ]
            }
        ],
        "Stop": [
            {
                "hooks": [
                    {
                        "type": "command",
                        "command": "~/.claude/hooks/loom-stop.sh"
                    }
                ]
            }
        ]
    })
}

/// Install the loom stop hook to ~/.claude/hooks/
///
/// This creates the hook script that enforces commit and stage completion
/// in loom worktrees before allowing Claude to stop.
///
/// Also removes any legacy flux-stop.sh hooks to prevent conflicts.
///
/// # Returns
/// - `Ok((installed, removed_legacy))` where:
///   - `installed` is true if the hook was installed or updated
///   - `removed_legacy` is true if legacy flux hooks were removed
/// - `Err` if installation failed
pub fn install_loom_hooks() -> Result<(bool, bool)> {
    // First, remove any legacy flux hooks
    let removed_legacy = remove_legacy_hooks()?;

    let home_dir = dirs::home_dir().context("Failed to determine home directory")?;
    let hooks_dir = home_dir.join(".claude/hooks");
    let hook_path = hooks_dir.join("loom-stop.sh");

    // Create hooks directory if needed
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir).with_context(|| {
            format!(
                "Failed to create hooks directory at {}",
                hooks_dir.display()
            )
        })?;
    }

    // Check if hook already exists with same content
    if hook_path.exists() {
        let existing_content = fs::read_to_string(&hook_path)
            .with_context(|| format!("Failed to read existing hook at {}", hook_path.display()))?;

        if existing_content == LOOM_STOP_HOOK {
            return Ok((false, removed_legacy)); // Already up to date
        }
    }

    // Write the hook script
    fs::write(&hook_path, LOOM_STOP_HOOK)
        .with_context(|| format!("Failed to write hook to {}", hook_path.display()))?;

    // Make executable (chmod +x)
    let mut perms = fs::metadata(&hook_path)
        .with_context(|| format!("Failed to get metadata for {}", hook_path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms)
        .with_context(|| format!("Failed to set permissions on {}", hook_path.display()))?;

    Ok((true, removed_legacy))
}

/// Remove duplicate hook entries from a hooks array
///
/// A duplicate is identified by having the same command in the hooks array.
/// This function modifies the array in place, keeping only the first occurrence
/// of each unique command.
fn remove_duplicate_hooks(hooks_arr: &mut Vec<Value>) {
    let mut seen_commands: std::collections::HashSet<String> = std::collections::HashSet::new();

    hooks_arr.retain(|hook_entry| {
        // Extract commands from this hook entry
        let commands: Vec<String> = hook_entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|hook| hook.get("command").and_then(|c| c.as_str()))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        // If any command in this entry has already been seen, remove the entry
        if commands.iter().any(|cmd| seen_commands.contains(cmd)) {
            return false;
        }

        // Mark these commands as seen
        for cmd in commands {
            seen_commands.insert(cmd);
        }

        true
    });
}

/// Check if a hook command already exists in a hooks array
fn hook_command_exists(hooks_arr: &[Value], command: &str) -> bool {
    hooks_arr.iter().any(|hook_entry| {
        hook_entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter().any(|hook| {
                    hook.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|cmd| cmd == command)
                })
            })
            .unwrap_or(false)
    })
}

/// Configure loom hooks in settings object
/// Returns true if hooks were added/updated, false if already configured
///
/// This function:
/// 1. Removes any duplicate hook entries before adding new ones
/// 2. Only adds hooks that don't already exist (by command)
/// 3. Handles both fresh configs and existing configs with duplicates
pub fn configure_loom_hooks(settings_obj: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let loom_hooks = loom_hooks_config();
    let mut modified = false;

    // Ensure hooks object exists
    let hooks = settings_obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks must be a JSON object"))?;

    // Process each hook type from loom config
    if let Some(loom_hooks_obj) = loom_hooks.as_object() {
        for (event_name, event_hooks) in loom_hooks_obj {
            let event_arr = hooks_obj
                .entry(event_name)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("hooks.{event_name} must be an array"))?;

            // First, remove any existing duplicates
            let original_len = event_arr.len();
            remove_duplicate_hooks(event_arr);
            if event_arr.len() != original_len {
                modified = true;
            }

            // Add loom hooks only if they don't already exist
            if let Some(new_hooks) = event_hooks.as_array() {
                for hook in new_hooks {
                    // Extract the command from this hook entry
                    let command = hook
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|h| h.get("command"))
                        .and_then(|c| c.as_str());

                    if let Some(cmd) = command {
                        if !hook_command_exists(event_arr, cmd) {
                            event_arr.push(hook.clone());
                            modified = true;
                        }
                    } else {
                        // No command found, add anyway (shouldn't happen with our config)
                        event_arr.push(hook.clone());
                        modified = true;
                    }
                }
            }
        }
    }

    Ok(modified)
}
