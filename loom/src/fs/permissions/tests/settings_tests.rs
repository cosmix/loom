//! Tests for settings functions

use crate::fs::permissions::settings::{create_worktree_settings, ensure_loom_permissions};
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_ensure_loom_permissions_creates_new_file() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    ensure_loom_permissions(repo_root).unwrap();

    let settings_path = repo_root.join(".claude/settings.json");
    assert!(settings_path.exists());

    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Bash(loom:*)"));
}

#[test]
fn test_ensure_loom_permissions_merges_existing() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // Create existing settings with some permissions
    let existing = json!({
        "permissions": {
            "allow": ["Read(src/**)"],
            "deny": ["Bash(rm -rf:*)"]
        },
        "other_setting": true
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions(repo_root).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Check existing permissions preserved
    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));

    // Check loom CLI permissions added
    assert!(allow.iter().any(|v| v == "Bash(loom:*)"));

    // Check deny list preserved
    let deny = settings["permissions"]["deny"].as_array().unwrap();
    assert!(deny.iter().any(|v| v == "Bash(rm -rf:*)"));

    // Check other settings preserved
    assert_eq!(settings["other_setting"], true);
}

#[test]
fn test_ensure_loom_permissions_no_duplicates() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // Create existing settings with some loom permissions already
    let existing = json!({
        "permissions": {
            "allow": ["Bash(loom:*)"]
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions(repo_root).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let allow = settings["permissions"]["allow"].as_array().unwrap();

    // Count occurrences of Bash(loom:*) - should be exactly 1
    let loom_count = allow.iter().filter(|v| *v == "Bash(loom:*)").count();
    assert_eq!(loom_count, 1);
}

#[test]
fn test_ensure_loom_permissions_adds_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    ensure_loom_permissions(repo_root).unwrap();

    let settings_path = repo_root.join(".claude/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Check hooks are configured
    let hooks = settings.get("hooks").expect("hooks should be present");
    let hooks_obj = hooks.as_object().unwrap();

    assert!(hooks_obj.contains_key("PreToolUse"));
    assert!(hooks_obj.contains_key("PostToolUse"));
    assert!(hooks_obj.contains_key("Stop"));
}

#[test]
fn test_hooks_not_duplicated_on_rerun() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // Run twice
    ensure_loom_permissions(repo_root).unwrap();
    ensure_loom_permissions(repo_root).unwrap();

    let settings_path = repo_root.join(".claude/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Should still have exactly one Stop hook entry
    let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop_hooks.len(), 1);
}

#[test]
fn test_worktree_settings_includes_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path();

    create_worktree_settings(worktree_path).unwrap();

    let settings_path = worktree_path.join(".claude/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Check hooks are present
    let hooks = settings.get("hooks").expect("hooks should be present");
    let hooks_obj = hooks.as_object().unwrap();
    assert!(hooks_obj.contains_key("Stop"));
}
