//! Ownership and reconciliation of individual hook handlers.

use serde_json::Value;
use std::path::Path;

use super::super::constants::LOOM_HOOKS;

pub(in crate::fs::permissions) fn executable(command: &str) -> &str {
    let command = command.trim_start();
    if let Some(rest) = command.strip_prefix('\'') {
        return rest.split('\'').next().unwrap_or("");
    }
    command.split_whitespace().next().unwrap_or("")
}

pub(in crate::fs::permissions) fn is_owned(
    command: &str,
    installed: Option<&Path>,
    home: Option<&Path>,
) -> bool {
    let path = executable(command);
    let installed = installed.map(|dir| format!("{}/", dir.display()));
    let home_dir = home.map(|dir| format!("{}/.claude/hooks/loom/", dir.display()));
    if [
        Some("~/.claude/hooks/loom/"),
        Some("$HOME/.claude/hooks/loom/"),
        installed.as_deref(),
        home_dir.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|prefix| path.starts_with(prefix))
    {
        return true;
    }
    let home_old = home.map(|dir| format!("{}/.claude/hooks/", dir.display()));
    let legacy = [
        Some("~/.claude/hooks/"),
        Some("$HOME/.claude/hooks/"),
        home_old.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|prefix| path.strip_prefix(prefix))
    .any(|name| LOOM_HOOKS.iter().any(|(script, _)| *script == name));
    legacy
}

pub(in crate::fs::permissions) fn remove_owned(
    rules: &mut Vec<Value>,
    installed: Option<&Path>,
) -> bool {
    let mut changed = false;
    rules.retain_mut(|rule| {
        let Some(handlers) = rule.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = handlers.len();
        handlers.retain(|handler| {
            !handler
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| is_owned(command, installed, dirs::home_dir().as_deref()))
        });
        if before != handlers.len() {
            changed = true;
            !handlers.is_empty()
        } else {
            true
        }
    });
    changed
}
