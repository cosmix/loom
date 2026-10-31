//! Detect Loom-owned registrations in project settings.

use serde_json::Value;
use std::fs;
use std::path::Path;

/// Check if settings.json still contains hooks that should be in settings.local.json
pub fn settings_json_has_hooks(repo_root: &Path) -> bool {
    let settings_path = repo_root.join(".claude/settings.json");
    if !settings_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let settings: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return false;
    };
    hooks.values().filter_map(Value::as_array).any(|rules| {
        let mut rules = rules.clone();
        super::super::hooks::ownership::remove_owned(
            &mut rules,
            crate::assets::install::claude_hooks_dir().as_deref(),
        )
    })
}
