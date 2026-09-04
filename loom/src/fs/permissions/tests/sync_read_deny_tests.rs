//! Tests for `sync_worktree_permissions`'s `Read(...)` deny handling.

use crate::fs::permissions::sync::sync_worktree_permissions;
use crate::fs::permissions::SyncResult;
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

/// Sync a worktree carrying an allow-side `Read(src/**)` grant and five
/// `Read(...)` denies of mixed shape (token-style, operator-style,
/// glob-style) against a fresh main repo. Returns the sync result and the
/// main repo's settings after the run.
fn sync_with_mixed_read_denies() -> (SyncResult, Value) {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)"],
            "deny": [
                "Read(../.work/admin.token)",
                "Read(.loom/work/user.token)",
                "Read(//home/x/src/*/.work/admin.token)",
                "Read(../doc/**)",
                "Read(secrets/**)"
            ]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    let main_settings_path = main_dir.path().join(".claude/settings.local.json");
    let content = fs::read_to_string(&main_settings_path).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    (result, main_settings)
}

/// No `Read(...)` deny, of any shape — token or operator's own — is ever
/// promoted out of a worktree; the allow-side `Read(...)` grant still syncs
/// normally.
#[test]
fn test_sync_never_promotes_read_denies() {
    let (result, _main_settings) = sync_with_mixed_read_denies();

    assert_eq!(result.deny_added, 0);
    assert_eq!(result.allow_added, 1);
}

/// The synced main settings carry the allow grant but no `Read(...)` deny at all.
#[test]
fn test_sync_read_denies_absent_from_main_settings() {
    let (_result, main_settings) = sync_with_mixed_read_denies();

    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));

    let deny = main_settings["permissions"]["deny"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !deny
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.starts_with("Read("))),
        "no Read(...) deny entry, of any shape, may reach the main repo's deny list, got: {deny:?}"
    );
}
