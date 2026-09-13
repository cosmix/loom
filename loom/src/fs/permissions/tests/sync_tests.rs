//! Tests for sync_worktree_permissions
//!
//! The fold-back no longer writes `<main_repo>/.claude/settings.local.json`;
//! it records portable allow rules into the loom-owned approved-permissions
//! list at `<state_root>/permissions/approved.json` instead (`approved.rs`).
//! Every fixture below creates the nested `.loom/work` state-root layout so
//! `state_root::resolve_state_root` has somewhere to resolve to, then reads
//! the recorded rules back through that same layout.

use crate::fs::permissions::approved::approved_path;
use crate::fs::permissions::sync::sync_worktree_permissions;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create `<main_dir>/.loom/work`, the nested state-root layout
/// `resolve_state_root` looks for, and return its canonical path — the root
/// the fold-back resolves to and `approved_allow` reads back from.
fn create_state_root(main_dir: &Path) -> PathBuf {
    let work = main_dir.join(".loom").join("work");
    fs::create_dir_all(&work).unwrap();
    work.canonicalize().unwrap()
}

/// Read back the `allow` array the fold-back recorded into the loom-owned
/// approved-permissions list at `state_root`.
fn approved_allow(state_root: &Path) -> Vec<String> {
    let content = fs::read_to_string(approved_path(state_root)).unwrap();
    let value: Value = serde_json::from_str(&content).unwrap();
    value["allow"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

/// Assert the fold-back never created the main repository's own local
/// settings file.
fn assert_main_settings_never_created(main_dir: &Path) {
    let path = main_dir.join(".claude/settings.local.json");
    assert!(!path.exists(), "sync must never create {}", path.display());
}

#[test]
fn test_sync_basic_permissions() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)", "Edit(tests/**)"],
            "deny": ["Bash(rm -rf:*)"]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // Allow rules are recorded; there is no destination left for a deny rule.
    assert_eq!(result.allow_added, 2);
    assert_eq!(result.deny_added, 0);

    let approved = approved_allow(&state_root);
    assert!(approved.iter().any(|v| v == "Read(src/**)"));
    assert!(approved.iter().any(|v| v == "Edit(tests/**)"));

    assert_main_settings_never_created(main_dir.path());
}

#[test]
fn test_sync_drops_inert_write_rules() {
    // Claude Code's file permission check consults only `Edit(path)` rules, so
    // a `Write(...)` entry recorded into the approved list would grant
    // nothing there either. Sync must not carry either allow-side
    // `Write(...)` rule over.
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Write(tests/**)", "Write(../../doc/plans/**)", "Edit(src/**)"],
            "deny": ["Write(~/.bashrc)", "Bash(rm -rf:*)"]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // Only the enforceable entry is recorded; both Write(...) allow rules
    // are inert.
    assert_eq!(result.allow_added, 1);
    assert_eq!(result.deny_added, 0);

    let approved = approved_allow(&state_root);
    assert_eq!(approved, vec!["Edit(src/**)".to_string()]);
    assert!(
        !approved.iter().any(|v| v.starts_with("Write(")),
        "no Write(...) rule may reach the approved-permissions list, got: {approved:?}"
    );

    assert_main_settings_never_created(main_dir.path());
}

#[test]
fn test_sync_transforms_worktree_paths() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    // Create worktree settings with regular, transformable, and
    // non-transformable permissions.
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": [
                "Read(src/**)",                      // regular - keep as-is
                "Edit(../../doc/plans/**)",          // transformable - becomes Edit(doc/plans/**)
                "Read(../../../.loom/work/**)",      // transformed, but names the state root - dropped
                "Edit(.worktrees/stage-1/**)",       // non-transformable - filtered out
                "Bash(cargo:*)"                      // regular - keep as-is
            ]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // Read(src/**), Edit(doc/plans/**) and Bash(cargo:*) reach the approved
    // list. Edit(.worktrees/stage-1/**) is filtered as non-transformable, and
    // Read(.loom/work/**) - though the path rewrite still produces it - is
    // dropped separately for naming the state root itself.
    assert_eq!(result.allow_added, 3);

    let approved = approved_allow(&state_root);
    assert!(approved.iter().any(|v| v == "Read(src/**)"));
    assert!(approved.iter().any(|v| v == "Bash(cargo:*)"));
    assert!(approved.iter().any(|v| v == "Edit(doc/plans/**)"));
    assert!(!approved.iter().any(|v| v.contains("../../")));
    assert!(!approved.iter().any(|v| v.contains(".worktrees/")));
    assert!(
        !approved.iter().any(|v| v.contains(".loom")),
        "a rule naming the state root must never reach the approved list, got: {approved:?}"
    );
}

#[test]
fn test_sync_deduplicates() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    // The main repo's own local settings already carries an approval Claude
    // Code recorded directly against it (not through the fold-back).
    let main_claude_dir = main_dir.path().join(".claude");
    fs::create_dir_all(&main_claude_dir).unwrap();
    let main_settings = json!({
        "permissions": { "allow": ["Read(src/**)"] }
    });
    fs::write(
        main_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&main_settings).unwrap(),
    )
    .unwrap();

    // The worktree approved the same rule, plus a new one.
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();
    let worktree_settings = json!({
        "permissions": { "allow": ["Read(src/**)", "Edit(tests/**)"] }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // The rule shared by both sources is recorded once.
    assert_eq!(result.allow_added, 2);

    let approved = approved_allow(&state_root);
    let read_count = approved.iter().filter(|v| *v == "Read(src/**)").count();
    assert_eq!(read_count, 1, "Read(src/**) should appear exactly once");
    assert!(approved.iter().any(|v| v == "Edit(tests/**)"));
    assert_eq!(approved.len(), 2);
}

#[test]
fn test_sync_missing_worktree_settings() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Don't create worktree settings file

    // Run sync - should succeed but add nothing
    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    assert_eq!(result.allow_added, 0);
    assert_eq!(result.deny_added, 0);
}

/// Formerly `test_sync_creates_main_settings`: with no destination file left
/// to create, the invariant becomes that sync never creates one.
#[test]
fn test_sync_never_creates_main_settings() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();
    let worktree_settings = json!({
        "permissions": { "allow": ["Read(src/**)"] }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    assert_main_settings_never_created(main_dir.path());

    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    assert_eq!(result.allow_added, 1);

    // The rule landed in the loom-owned approved list, not a main settings file.
    assert_main_settings_never_created(main_dir.path());
    let approved = approved_allow(&state_root);
    assert!(approved.iter().any(|v| v == "Read(src/**)"));
}

/// Formerly `test_sync_preserves_other_fields`: with the main settings file
/// never written at all, the invariant becomes byte-for-byte identity.
#[test]
fn test_sync_leaves_main_settings_byte_identical() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    // Create main settings with other fields
    let main_claude_dir = main_dir.path().join(".claude");
    fs::create_dir_all(&main_claude_dir).unwrap();
    let main_settings = json!({
        "permissions": { "allow": ["Read(existing/**)"] },
        "hooks": { "PreToolUse": [] },
        "custom_field": "preserved",
        "nested": { "key": "value" }
    });
    let main_settings_path = main_claude_dir.join("settings.local.json");
    let original_content = serde_json::to_string_pretty(&main_settings).unwrap();
    fs::write(&main_settings_path, &original_content).unwrap();

    // Create worktree settings with new permissions
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();
    let worktree_settings = json!({
        "permissions": { "allow": ["Read(src/**)"] }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // The main settings file, custom fields and all, is untouched byte for byte.
    assert_eq!(
        fs::read_to_string(&main_settings_path).unwrap(),
        original_content
    );

    // Both the pre-existing and the new rule reached the approved list instead.
    let approved = approved_allow(&state_root);
    assert!(approved.iter().any(|v| v == "Read(existing/**)"));
    assert!(approved.iter().any(|v| v == "Read(src/**)"));
}

#[test]
fn test_sync_idempotent() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());

    // Create worktree settings
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)", "Edit(tests/**)"],
            "deny": ["Bash(rm -rf:*)"]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    // Run sync twice
    let result1 = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();
    let result2 = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // First sync should add permissions
    assert_eq!(result1.allow_added, 2);
    assert_eq!(result1.deny_added, 0);

    // Second sync should add nothing (idempotent)
    assert_eq!(result2.allow_added, 0);
    assert_eq!(result2.deny_added, 0);

    // Verify final state has no duplicates
    let approved = approved_allow(&state_root);
    assert_eq!(approved.len(), 2);
}
