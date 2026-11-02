//! Ownership and reconciliation of individual hook handlers.

use serde_json::Value;
use std::borrow::Cow;
use std::path::Path;

/// The only scripts Loom ever registered directly under `~/.claude/hooks/`.
/// Every later hook lives under `hooks/loom/`; a user's own script at the top
/// level is theirs even when it shares a shipped hook's name. A user's script
/// with one of these three names cannot be told apart from Loom's old one.
const LEGACY_TOP_LEVEL_HOOKS: &[&str] = &["commit-guard.sh", "ask-user-pre.sh", "ask-user-post.sh"];

/// The command's first shell word. A single-quoted word may hold spaces and
/// the `'\X'` spelling `shell_escape` gives an apostrophe or `!`; anything else
/// ends at whitespace.
pub(in crate::fs::permissions) fn executable(command: &str) -> Cow<'_, str> {
    let command = command.trim_start();
    let Some(mut quoted) = command.strip_prefix('\'') else {
        return Cow::Borrowed(command.split_whitespace().next().unwrap_or(""));
    };
    let mut word = String::new();
    loop {
        let Some((part, rest)) = quoted.split_once('\'') else {
            word.push_str(quoted);
            return Cow::Owned(word);
        };
        word.push_str(part);
        let mut after_escape = rest.strip_prefix('\\').map(str::chars);
        let escaped = after_escape.as_mut().and_then(Iterator::next);
        let reopened = after_escape.and_then(|chars| chars.as_str().strip_prefix('\''));
        let (Some(escaped), Some(reopened)) = (escaped, reopened) else {
            return Cow::Owned(word);
        };
        word.push(escaped);
        quoted = reopened;
    }
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
    .any(|name| LEGACY_TOP_LEVEL_HOOKS.contains(&name));
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
