//! Tests for `sync_worktree_permissions`'s `Read(...)` deny handling.
//!
//! The fold-back records only allow rules into the loom-owned
//! approved-permissions list; it no longer writes a main-repo settings file
//! for a `Read(...)` deny to leak into.

use crate::fs::permissions::approved::approved_path;
use crate::fs::permissions::sync::sync_worktree_permissions;
use crate::fs::permissions::SyncResult;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create `<main_dir>/.loom/work`, the nested state-root layout
/// `resolve_state_root` looks for, and return its canonical path.
fn create_state_root(main_dir: &Path) -> PathBuf {
    let work = main_dir.join(".loom").join("work");
    fs::create_dir_all(&work).unwrap();
    work.canonicalize().unwrap()
}

/// Sync a worktree carrying an allow-side `Read(src/**)` grant and five
/// `Read(...)` denies of mixed shape (token-style, operator-style,
/// glob-style) against a fresh main repo. Returns the sync result and the
/// loom-owned approved-permissions list the fold-back recorded.
fn sync_with_mixed_read_denies() -> (SyncResult, Value) {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

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

    let content = fs::read_to_string(approved_path(&state_root)).unwrap();
    let approved: Value = serde_json::from_str(&content).unwrap();

    (result, approved)
}

/// No `Read(...)` deny, of any shape — token or operator's own — is ever
/// promoted out of a worktree; the allow-side `Read(...)` grant still syncs
/// normally.
#[test]
fn test_sync_never_promotes_read_denies() {
    let (result, _approved) = sync_with_mixed_read_denies();

    assert_eq!(result.deny_added, 0);
    assert_eq!(result.allow_added, 1);
}

/// The loom-owned approved-permissions list carries the allow grant but no
/// `Read(...)` deny at all — the list only ever holds allow rules, so there
/// is no `deny` array for one to hide in either.
#[test]
fn test_sync_read_denies_absent_from_main_settings() {
    let (_result, approved) = sync_with_mixed_read_denies();

    let allow = approved["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
    assert!(
        approved.get("deny").is_none(),
        "the approved-permissions list must carry no deny array, got: {approved:?}"
    );
    assert!(
        !allow
            .iter()
            .any(|v| v.as_str().is_some_and(|s| s.starts_with("Read(") && s != "Read(src/**)")),
        "no Read(...) deny entry, of any shape, may reach the approved-permissions list, got: {allow:?}"
    );
}
