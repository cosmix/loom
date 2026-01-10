//! Hook configuration and installation for loom

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::constants::LOOM_STOP_HOOK;

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
/// # Returns
/// - `Ok(true)` if the hook was installed or updated
/// - `Ok(false)` if the hook already exists and is up to date
/// - `Err` if installation failed
pub fn install_loom_hooks() -> Result<bool> {
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
            return Ok(false); // Already up to date
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

    Ok(true)
}

/// Configure loom hooks in settings object
/// Returns true if hooks were added/updated, false if already configured
pub fn configure_loom_hooks(settings_obj: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let loom_hooks = loom_hooks_config();

    // Check if hooks already exist
    if let Some(existing_hooks) = settings_obj.get("hooks") {
        // Check if loom hooks are already configured by looking for our specific hooks
        if let Some(hooks_obj) = existing_hooks.as_object() {
            // Check for Stop hook with loom-stop.sh as marker
            if let Some(stop_hooks) = hooks_obj.get("Stop") {
                if let Some(stop_arr) = stop_hooks.as_array() {
                    for hook_entry in stop_arr {
                        if let Some(hooks) = hook_entry.get("hooks").and_then(|h| h.as_array()) {
                            for hook in hooks {
                                if let Some(cmd) = hook.get("command").and_then(|c| c.as_str()) {
                                    if cmd.contains("loom-stop.sh") {
                                        // Loom hooks already configured
                                        return Ok(false);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Merge loom hooks into existing hooks or create new
    let hooks = settings_obj.entry("hooks").or_insert_with(|| json!({}));

    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks must be a JSON object"))?;

    // Add each hook type from loom config
    if let Some(loom_hooks_obj) = loom_hooks.as_object() {
        for (event_name, event_hooks) in loom_hooks_obj {
            let event_arr = hooks_obj
                .entry(event_name)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("hooks.{event_name} must be an array"))?;

            // Add loom hooks to the array
            if let Some(new_hooks) = event_hooks.as_array() {
                for hook in new_hooks {
                    event_arr.push(hook.clone());
                }
            }
        }
    }

    Ok(true)
}
