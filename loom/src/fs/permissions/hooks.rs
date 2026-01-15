//! Hook configuration and installation for loom

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::constants::LOOM_HOOKS;

/// Generate hooks configuration for loom
/// Hooks reference scripts at ~/.claude/hooks/loom/ (installed by loom init)
pub fn loom_hooks_config() -> Value {
    // Get home directory for full path expansion (~ may not work in all contexts)
    // All hooks are in the loom/ subdirectory to separate from user hooks
    let hooks_dir = dirs::home_dir()
        .map(|h| h.join(".claude/hooks/loom").display().to_string())
        .unwrap_or_else(|| "~/.claude/hooks/loom".to_string());

    json!({
        "PreToolUse": [
            {
                "matcher": "AskUserQuestion",
                "hooks": [
                    {
                        "type": "command",
                        "command": format!("{}/ask-user-pre.sh", hooks_dir)
                    }
                ]
            },
            {
                "matcher": "Bash",
                "hooks": [
                    {
                        "type": "command",
                        "command": format!("{}/prefer-modern-tools.sh", hooks_dir)
                    }
                ]
            }
        ],
        "PostToolUse": [
            {
                "matcher": "Bash",
                "hooks": [
                    {
                        "type": "command",
                        "command": format!("{}/post-tool-use.sh", hooks_dir)
                    }
                ]
            },
            {
                "matcher": "AskUserQuestion",
                "hooks": [
                    {
                        "type": "command",
                        "command": format!("{}/ask-user-post.sh", hooks_dir)
                    }
                ]
            }
        ],
        "Stop": [
            {
                "matcher": "*",
                "hooks": [
                    {
                        "type": "command",
                        "command": format!("{}/commit-guard.sh", hooks_dir)
                    }
                ]
            }
        ],
        "UserPromptSubmit": [
            {
                "matcher": "*",
                "hooks": [
                    {
                        "type": "command",
                        "command": format!("{}/skill-trigger.sh", hooks_dir)
                    }
                ]
            }
        ]
    })
}

/// Install all loom hooks to ~/.claude/hooks/loom/
///
/// All hooks are installed to the loom/ subdirectory to keep them
/// separate from any user-defined hooks.
///
/// # Returns
/// - `Ok(count)` - number of hooks installed or updated
/// - `Err` if installation failed
pub fn install_loom_hooks() -> Result<usize> {
    let home_dir = dirs::home_dir().context("Failed to determine home directory")?;
    let hooks_dir = home_dir.join(".claude/hooks/loom");

    // Create hooks directory if needed
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir).with_context(|| {
            format!(
                "Failed to create hooks directory at {}",
                hooks_dir.display()
            )
        })?;
    }

    let mut installed_count = 0;

    // Install all hooks to ~/.claude/hooks/loom/
    for (filename, content) in LOOM_HOOKS {
        if install_hook_script(&hooks_dir, filename, content)? {
            installed_count += 1;
        }
    }

    Ok(installed_count)
}

/// Install a single hook script to a directory
///
/// Returns true if the hook was installed or updated, false if already up to date.
fn install_hook_script(dir: &std::path::Path, filename: &str, content: &str) -> Result<bool> {
    let hook_path = dir.join(filename);

    // Check if hook already exists with same content
    if hook_path.exists() {
        let existing_content = fs::read_to_string(&hook_path)
            .with_context(|| format!("Failed to read existing hook at {}", hook_path.display()))?;

        if existing_content == content {
            return Ok(false); // Already up to date
        }
    }

    // Write the hook script
    fs::write(&hook_path, content)
        .with_context(|| format!("Failed to write hook to {}", hook_path.display()))?;

    // Make executable (chmod +x)
    let mut perms = fs::metadata(&hook_path)
        .with_context(|| format!("Failed to get metadata for {}", hook_path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms)
        .with_context(|| format!("Failed to set permissions on {}", hook_path.display()))?;

    Ok(true)
}

/// Get the path to the installed worktree hooks directory
///
/// Returns ~/.claude/hooks/loom/ where worktree hook scripts are installed.
pub fn get_installed_hooks_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude/hooks/loom"))
}

/// Migrate old hook paths to the new loom subdirectory structure
///
/// Scans hooks config for paths starting with `~/.claude/hooks/` but NOT
/// `~/.claude/hooks/loom/` and updates them to use the new loom subdirectory.
///
/// This handles migration from the old hook location:
///   `~/.claude/hooks/commit-guard.sh` -> `~/.claude/hooks/loom/commit-guard.sh`
///
/// Returns `true` if any paths were migrated, `false` otherwise.
fn migrate_old_hook_paths(settings_obj: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let home_dir = dirs::home_dir().context("Failed to determine home directory")?;
    let old_prefix = home_dir.join(".claude/hooks");
    let new_prefix = home_dir.join(".claude/hooks/loom");

    // Also handle tilde-prefixed paths
    let old_tilde_prefix = "~/.claude/hooks/";
    let new_tilde_prefix = "~/.claude/hooks/loom/";

    let mut migrated = false;

    // Get or return early if no hooks
    let hooks = match settings_obj.get_mut("hooks") {
        Some(h) => h,
        None => return Ok(false),
    };

    let hooks_obj = match hooks.as_object_mut() {
        Some(obj) => obj,
        None => return Ok(false),
    };

    // Process each hook type
    for (_event_name, event_hooks) in hooks_obj.iter_mut() {
        let event_arr = match event_hooks.as_array_mut() {
            Some(arr) => arr,
            None => continue,
        };

        for hook_entry in event_arr.iter_mut() {
            let hooks_inner = match hook_entry.get_mut("hooks") {
                Some(h) => h,
                None => continue,
            };

            let hooks_arr = match hooks_inner.as_array_mut() {
                Some(arr) => arr,
                None => continue,
            };

            for hook in hooks_arr.iter_mut() {
                let cmd = match hook.get("command").and_then(|c| c.as_str()) {
                    Some(c) => c.to_string(),
                    None => continue,
                };

                // Check if this is an old-style path that needs migration
                // Try tilde-prefixed path first
                let new_cmd = if let Some(rest) = cmd.strip_prefix(old_tilde_prefix) {
                    // Only migrate if not already in loom/ subdirectory
                    if rest.starts_with("loom/") {
                        None
                    } else {
                        Some(format!("{new_tilde_prefix}{rest}"))
                    }
                } else if let Ok(stripped) = std::path::Path::new(&cmd).strip_prefix(&old_prefix) {
                    // Absolute path
                    let first_component = stripped.components().next();
                    let needs_migration = match first_component {
                        Some(std::path::Component::Normal(name)) => name != "loom",
                        _ => true, // Empty path or other component type
                    };
                    if needs_migration {
                        Some(new_prefix.join(stripped).display().to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(new_cmd) = new_cmd {
                    // Update the command in place
                    if let Some(hook_obj) = hook.as_object_mut() {
                        hook_obj.insert("command".to_string(), json!(new_cmd));
                        migrated = true;
                    }
                }
            }
        }
    }

    Ok(migrated)
}

/// Extract the basename (filename) from a command path
///
/// Returns the filename component of a path, or the original string if extraction fails.
fn extract_command_basename(cmd: &str) -> String {
    std::path::Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd)
        .to_string()
}

/// Remove duplicate hook entries from a hooks array
///
/// A duplicate is identified by having the same script basename in the hooks array.
/// This allows deduplication even when the same script exists at different paths
/// (e.g., `~/.claude/hooks/commit-guard.sh` vs `~/.claude/hooks/loom/commit-guard.sh`).
/// This function modifies the array in place, keeping only the first occurrence
/// of each unique command basename.
fn remove_duplicate_hooks(hooks_arr: &mut Vec<Value>) {
    let mut seen_basenames: std::collections::HashSet<String> = std::collections::HashSet::new();

    hooks_arr.retain(|hook_entry| {
        // Extract basenames from this hook entry
        let basenames: Vec<String> = hook_entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|hook| hook.get("command").and_then(|c| c.as_str()))
                    .map(extract_command_basename)
                    .collect()
            })
            .unwrap_or_default();

        // If any basename in this entry has already been seen, remove the entry
        if basenames.iter().any(|bn| seen_basenames.contains(bn)) {
            return false;
        }

        // Mark these basenames as seen
        for bn in basenames {
            seen_basenames.insert(bn);
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
/// 1. Migrates old hook paths to the new loom/ subdirectory
/// 2. Removes any duplicate hook entries before adding new ones
/// 3. Only adds hooks that don't already exist (by command)
/// 4. Handles both fresh configs and existing configs with duplicates
pub fn configure_loom_hooks(settings_obj: &mut serde_json::Map<String, Value>) -> Result<bool> {
    // First, migrate any old hook paths to the new loom/ subdirectory
    let migrated = migrate_old_hook_paths(settings_obj)?;
    let mut modified = migrated;

    let loom_hooks = loom_hooks_config();

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
