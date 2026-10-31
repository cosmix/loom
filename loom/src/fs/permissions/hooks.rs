//! Hook configuration and installation for loom

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::constants::LOOM_HOOKS;
use super::drift;

#[path = "hooks/ownership.rs"]
pub(super) mod ownership;

#[path = "hooks/config.rs"]
mod config;

/// Generate global hooks configuration for a specific hooks directory.
/// `pub(super)`: also used by [`loom_hooks_config`] and by [`super::drift`]
/// to build the canonical config a settings document is compared against.
pub(super) fn loom_hooks_config_for_dir(hooks_dir: &str) -> Value {
    config::build(hooks_dir)
}

/// Generate global Loom registrations installed in the main repository's
/// settings.local.json; session capsules add session-specific registrations.
pub fn loom_hooks_config() -> Value {
    loom_hooks_config_for_dir(&crate::assets::install::claude_hooks_dir_text())
}

/// Install all loom hooks to the installed hooks directory
/// (`assets::install::claude_hooks_dir`). Use `install_loom_hooks_to` for
/// testability with custom directories.
pub fn install_loom_hooks() -> Result<usize> {
    let hooks_dir = crate::assets::install::try_claude_hooks_dir()
        .context("Failed to determine the loom hooks directory")?;
    install_loom_hooks_to(&hooks_dir)
}

/// Install all loom hooks to a specific directory
///
/// All hooks are installed to the given directory. In production this is
/// `~/.claude/hooks/loom/`; tests pass a temp directory instead.
///
/// # Returns
/// - `Ok(count)` - number of hooks installed or updated
/// - `Err` if installation failed
pub fn install_loom_hooks_to(hooks_dir: &std::path::Path) -> Result<usize> {
    // Create hooks directory if needed
    if !hooks_dir.exists() {
        fs::create_dir_all(hooks_dir).with_context(|| {
            format!(
                "Failed to create hooks directory at {}",
                hooks_dir.display()
            )
        })?;
    }

    let mut installed_count = 0;

    for (filename, content) in LOOM_HOOKS {
        if install_hook_script(hooks_dir, filename, content)? {
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

    // Shares its "up to date" definition with `hook_scripts_needing_install`
    // so the installer and the drift detector can never disagree.
    if drift::hook_script_is_current(dir, filename, content) {
        return Ok(false); // Already up to date
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

/// Extract the basename (filename) from a command path, or the original
/// string if extraction fails. `pub(super)`: also used by [`super::drift`]
/// to compare registrations against `LOOM_HOOKS`.
pub(super) fn extract_command_basename(cmd: &str) -> String {
    std::path::Path::new(ownership::executable(cmd))
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd)
        .to_string()
}

/// Check ownership against an explicit `hooks_dir` and `home`. Each spelling
/// must start the command: a project's own `<repo>/.claude/hooks/loom/` hook,
/// or any other tree that merely holds `hooks/loom/`, is the user's.
pub(super) fn is_loom_hook_in(
    command: &str,
    hooks_dir: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
) -> bool {
    ownership::is_owned(command, hooks_dir, home)
}

/// Remove all loom hooks from a hooks array
///
/// This removes any hook entries that point to scripts in ~/.claude/hooks/loom/
/// so they can be replaced with the current loom hook configuration.
#[cfg(test)]
fn remove_loom_hooks(hooks_arr: &mut Vec<Value>) {
    ownership::remove_owned(
        hooks_arr,
        crate::assets::install::claude_hooks_dir().as_deref(),
    );
}

/// Apply a loom hooks config (built from `hooks_dir`) to a settings object.
///
/// Shared logic for [`configure_loom_hooks`]:
/// Replaces owned handlers with fresh registrations from the given directory.
///
/// User's non-loom hooks are preserved. `pub(super)`: also exercised
/// directly by [`super::drift`]'s tests against a specific `hooks_dir`.
pub(super) fn configure_loom_hooks_with_dir(
    settings_obj: &mut serde_json::Map<String, Value>,
    hooks_dir: &str,
) -> Result<bool> {
    let loom_hooks = loom_hooks_config_for_dir(hooks_dir);

    let hooks = settings_obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks_obj = drift::coerce_container(hooks, || json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("hooks must be a JSON object"))?;

    let mut all_event_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for key in hooks_obj.keys() {
        all_event_names.insert(key.clone());
    }
    if let Some(loom_hooks_obj) = loom_hooks.as_object() {
        for key in loom_hooks_obj.keys() {
            all_event_names.insert(key.clone());
        }
    }

    let loom_hooks_obj = loom_hooks.as_object();
    for event_name in all_event_names {
        let event_val = hooks_obj.entry(&event_name).or_insert_with(|| json!([]));
        let event_arr = drift::coerce_container(event_val, || json!([]))
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks.{event_name} must be an array"))?;

        ownership::remove_owned(event_arr, Some(std::path::Path::new(hooks_dir)));

        if let Some(new_hooks) = loom_hooks_obj
            .and_then(|obj| obj.get(&event_name))
            .and_then(|h| h.as_array())
        {
            for hook in new_hooks {
                event_arr.push(hook.clone());
            }
        }
    }

    Ok(true)
}

/// Configure loom hooks in settings object using host paths.
///
/// Hook scripts are accessed in the installed hooks directory
/// (`assets::install::claude_hooks_dir`) on the host filesystem.
///
/// Returns true if hooks were added/updated.
pub fn configure_loom_hooks(settings_obj: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let hooks_dir = crate::assets::install::claude_hooks_dir_text();
    configure_loom_hooks_with_dir(settings_obj, &hooks_dir)
}

#[cfg(test)]
#[path = "hooks/registration_tests.rs"]
mod registration_tests;

#[cfg(all(test, unix))]
#[path = "hooks/policy_tests.rs"]
mod policy_tests;
