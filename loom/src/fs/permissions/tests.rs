//! Tests for permissions module

use super::constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE, LOOM_STOP_HOOK};
use super::hooks::{install_loom_hooks, loom_hooks_config};
use super::settings::{create_worktree_settings, ensure_loom_permissions};
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_ensure_loom_permissions_creates_new_file() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    ensure_loom_permissions(repo_root).unwrap();

    let settings_path = repo_root.join(".claude/settings.local.json");
    assert!(settings_path.exists());

    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Bash(loom:*)"));
    assert!(allow.iter().any(|v| v == "Bash(tmux:*)"));
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
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions(repo_root).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Check existing permissions preserved
    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));

    // Check loom CLI permissions added
    assert!(allow.iter().any(|v| v == "Bash(loom:*)"));
    assert!(allow.iter().any(|v| v == "Bash(tmux:*)"));

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
            "allow": ["Bash(loom:*)", "Bash(tmux:*)"]
        }
    });
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions(repo_root).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let allow = settings["permissions"]["allow"].as_array().unwrap();

    // Count occurrences of Bash(loom:*) - should be exactly 1
    let loom_count = allow.iter().filter(|v| *v == "Bash(loom:*)").count();
    assert_eq!(loom_count, 1);
}

#[test]
fn test_loom_permissions_constant() {
    // Main repo includes all permissions (shared with worktrees via symlink)
    assert!(LOOM_PERMISSIONS.contains(&"Bash(loom:*)"));
    assert!(LOOM_PERMISSIONS.contains(&"Bash(tmux:*)"));
    // Now includes worktree permissions so settings can be symlinked
    assert!(LOOM_PERMISSIONS.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Write(.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Read(../../.work/**)"));
    assert!(LOOM_PERMISSIONS.contains(&"Write(../../.work/**)"));
}

#[test]
fn test_worktree_permissions_constant() {
    // Worktree permissions should match main repo permissions
    // (since settings.local.json is now symlinked from main)
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(../../.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Write(../../.work/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(.claude/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Read(~/.claude/**)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(loom:*)"));
    assert!(LOOM_PERMISSIONS_WORKTREE.contains(&"Bash(tmux:*)"));
}

#[test]
fn test_hooks_config_structure() {
    let hooks = loom_hooks_config();
    let hooks_obj = hooks.as_object().unwrap();

    // Check PreToolUse hook
    let pre_tool = hooks_obj.get("PreToolUse").unwrap().as_array().unwrap();
    assert_eq!(pre_tool.len(), 1);
    assert_eq!(pre_tool[0]["matcher"], "AskUserQuestion");
    assert!(pre_tool[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("ask-user-pre.sh"));

    // Check PostToolUse hook
    let post_tool = hooks_obj.get("PostToolUse").unwrap().as_array().unwrap();
    assert_eq!(post_tool.len(), 1);
    assert_eq!(post_tool[0]["matcher"], "AskUserQuestion");
    assert!(post_tool[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("ask-user-post.sh"));

    // Check Stop hook
    let stop = hooks_obj.get("Stop").unwrap().as_array().unwrap();
    assert_eq!(stop.len(), 1);
    assert!(stop[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("loom-stop.sh"));
}

#[test]
fn test_ensure_loom_permissions_adds_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    ensure_loom_permissions(repo_root).unwrap();

    let settings_path = repo_root.join(".claude/settings.local.json");
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

    let settings_path = repo_root.join(".claude/settings.local.json");
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

    let settings_path = worktree_path.join(".claude/settings.local.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Check hooks are present
    let hooks = settings.get("hooks").expect("hooks should be present");
    let hooks_obj = hooks.as_object().unwrap();
    assert!(hooks_obj.contains_key("Stop"));
}

#[test]
fn test_embedded_loom_stop_hook_is_valid() {
    // Verify the embedded hook script has correct shebang and key functions
    assert!(LOOM_STOP_HOOK.starts_with("#!/usr/bin/env bash"));
    // Check for key functions that should exist
    assert!(LOOM_STOP_HOOK.contains("detect_loom_worktree"));
    assert!(LOOM_STOP_HOOK.contains("check_git_clean"));
    assert!(LOOM_STOP_HOOK.contains("get_stage_status"));
    assert!(LOOM_STOP_HOOK.contains("block_with_reason"));
}

#[test]
fn test_install_loom_hooks_creates_hook_file() {
    // This test modifies ~/.claude/hooks/ which is a real directory
    // We'll verify the hook content matches what we expect
    let result = install_loom_hooks();
    assert!(result.is_ok());

    let home_dir = dirs::home_dir().expect("should have home dir");
    let hook_path = home_dir.join(".claude/hooks/loom-stop.sh");

    if hook_path.exists() {
        let content = fs::read_to_string(&hook_path).unwrap();
        // Verify the hook has the expected content
        assert!(content.contains("detect_loom_worktree"));
        assert!(content.contains("LOOM WORKTREE EXIT BLOCKED"));
    }
}
