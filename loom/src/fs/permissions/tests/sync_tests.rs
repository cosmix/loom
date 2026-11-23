//! Tests for `fold_back_approvals`
//!
//! The fold-back reads the main checkout's own
//! `<main_repo>/.claude/settings.local.json`, never writes it, and records
//! portable allow rules into the loom-owned approved-permissions list at
//! `<state_root>/permissions/approved.json` (`approved.rs`). Every fixture
//! below creates the nested `.loom/work` state-root layout so
//! `state_root::resolve_state_root` has somewhere to resolve to, then reads
//! the recorded rules back through that same layout.

use crate::fs::permissions::approved::approved_path;
use crate::fs::permissions::fold_back_approvals;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create `<main_dir>/.loom/work`, the nested state-root layout
/// `resolve_state_root` looks for, and return its canonical path — the root
/// the fold-back resolves to and `approved_allow` reads back from.
pub(super) fn create_state_root(main_dir: &Path) -> PathBuf {
    let work = main_dir.join(".loom").join("work");
    fs::create_dir_all(&work).unwrap();
    work.canonicalize().unwrap()
}

/// Read back the `allow` array the fold-back recorded into the loom-owned
/// approved-permissions list at `state_root`.
pub(super) fn approved_allow(state_root: &Path) -> Vec<String> {
    let content = fs::read_to_string(approved_path(state_root)).unwrap();
    let value: Value = serde_json::from_str(&content).unwrap();
    value["allow"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

/// Write `settings` as the main checkout's local settings file.
fn write_main_settings(main_dir: &Path, settings: &Value) -> PathBuf {
    let claude_dir = main_dir.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let path = claude_dir.join("settings.local.json");
    fs::write(&path, serde_json::to_string_pretty(settings).unwrap()).unwrap();
    path
}

#[test]
fn test_sync_basic_permissions() {
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    write_main_settings(
        main_dir.path(),
        &json!({
            "permissions": {
                "allow": ["Read(src/**)", "Edit(tests/**)"],
                "deny": ["Bash(rm -rf:*)"]
            }
        }),
    );

    assert_eq!(fold_back_approvals(main_dir.path()), 2);

    let approved = approved_allow(&state_root);
    assert!(approved.iter().any(|v| v == "Read(src/**)"));
    assert!(approved.iter().any(|v| v == "Edit(tests/**)"));
}

#[test]
fn test_sync_drops_inert_write_rules() {
    // Claude Code's file permission check consults only `Edit(path)` rules, so
    // a `Write(...)` entry recorded into the approved list would grant
    // nothing there either. The fold-back must not carry an allow-side
    // `Write(...)` rule over.
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    write_main_settings(
        main_dir.path(),
        &json!({
            "permissions": {
                "allow": ["Write(tests/**)", "Write(../../doc/plans/**)", "Edit(src/**)"],
                "deny": ["Write(~/.bashrc)", "Bash(rm -rf:*)"]
            }
        }),
    );

    // Only the enforceable entry is recorded; both Write(...) allow rules
    // are inert.
    assert_eq!(fold_back_approvals(main_dir.path()), 1);

    let approved = approved_allow(&state_root);
    assert_eq!(approved, vec!["Edit(src/**)".to_string()]);
    assert!(
        !approved.iter().any(|v| v.starts_with("Write(")),
        "no Write(...) rule may reach the approved-permissions list, got: {approved:?}"
    );
}

#[test]
fn test_sync_transforms_worktree_paths() {
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    write_main_settings(
        main_dir.path(),
        &json!({
            "permissions": {
                "allow": [
                    "Read(src/**)",                      // regular - keep as-is
                    "Edit(../../doc/plans/**)",          // transformable - becomes Edit(doc/plans/**)
                    "Read(../../../.loom/work/**)",      // transformed, but names the state root - dropped
                    "Edit(.worktrees/stage-1/**)",       // non-transformable - filtered out
                    "Bash(cargo:*)"                      // regular - keep as-is
                ]
            }
        }),
    );

    // Read(src/**), Edit(doc/plans/**) and Bash(cargo:*) reach the approved
    // list. Edit(.worktrees/stage-1/**) is filtered as non-transformable, and
    // Read(.loom/work/**) - though the path rewrite still produces it - is
    // dropped separately for naming the state root itself.
    assert_eq!(fold_back_approvals(main_dir.path()), 3);

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
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    write_main_settings(
        main_dir.path(),
        &json!({
            "permissions": { "allow": ["Read(src/**)", "Edit(tests/**)", "Read(src/**)"] }
        }),
    );

    // The rule listed twice is recorded once.
    assert_eq!(fold_back_approvals(main_dir.path()), 2);

    let approved = approved_allow(&state_root);
    let read_count = approved.iter().filter(|v| *v == "Read(src/**)").count();
    assert_eq!(read_count, 1, "Read(src/**) should appear exactly once");
    assert!(approved.iter().any(|v| v == "Edit(tests/**)"));
    assert_eq!(approved.len(), 2);
}

#[test]
fn test_sync_missing_main_settings() {
    let main_dir = TempDir::new().unwrap();
    create_state_root(main_dir.path());

    assert_eq!(fold_back_approvals(main_dir.path()), 0);
}

#[test]
fn test_sync_never_creates_main_settings() {
    let main_dir = TempDir::new().unwrap();
    create_state_root(main_dir.path());
    let path = main_dir.path().join(".claude/settings.local.json");

    assert_eq!(fold_back_approvals(main_dir.path()), 0);

    assert!(
        !path.exists(),
        "the fold-back must never create {}",
        path.display()
    );
}

#[test]
fn test_sync_leaves_main_settings_byte_identical() {
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    let main_settings_path = write_main_settings(
        main_dir.path(),
        &json!({
            "permissions": { "allow": ["Read(existing/**)"] },
            "hooks": { "PreToolUse": [] },
            "custom_field": "preserved",
            "nested": { "key": "value" }
        }),
    );
    let original_content = fs::read_to_string(&main_settings_path).unwrap();

    assert_eq!(fold_back_approvals(main_dir.path()), 1);

    // The main settings file, custom fields and all, is untouched byte for byte.
    assert_eq!(
        fs::read_to_string(&main_settings_path).unwrap(),
        original_content
    );
    let approved = approved_allow(&state_root);
    assert!(approved.iter().any(|v| v == "Read(existing/**)"));
}

#[test]
fn test_sync_idempotent() {
    let main_dir = TempDir::new().unwrap();
    let state_root = create_state_root(main_dir.path());
    write_main_settings(
        main_dir.path(),
        &json!({
            "permissions": {
                "allow": ["Read(src/**)", "Edit(tests/**)"],
                "deny": ["Bash(rm -rf:*)"]
            }
        }),
    );

    assert_eq!(fold_back_approvals(main_dir.path()), 2);
    assert_eq!(fold_back_approvals(main_dir.path()), 0);

    let approved = approved_allow(&state_root);
    assert_eq!(approved, vec!["Edit(tests/**)", "Read(src/**)"]);
}
