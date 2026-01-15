//! Tests for permissions module

use super::constants::{HOOK_COMMIT_GUARD, LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
use super::hooks::{install_loom_hooks, loom_hooks_config};
use super::settings::{create_worktree_settings, ensure_loom_permissions};
use super::sync::sync_worktree_permissions;
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
}

#[test]
fn test_hooks_config_structure() {
    let hooks = loom_hooks_config();
    let hooks_obj = hooks.as_object().unwrap();

    // Check PreToolUse hooks (AskUserQuestion for stage status, Bash for prefer-modern-tools)
    let pre_tool = hooks_obj.get("PreToolUse").unwrap().as_array().unwrap();
    assert_eq!(pre_tool.len(), 2);
    // First hook: AskUserQuestion matcher with ask-user-pre.sh
    assert_eq!(pre_tool[0]["matcher"], "AskUserQuestion");
    assert!(pre_tool[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("ask-user-pre.sh"));
    // Second hook: Bash matcher with prefer-modern-tools.sh
    assert_eq!(pre_tool[1]["matcher"], "Bash");
    assert!(pre_tool[1]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("prefer-modern-tools.sh"));

    // Check PostToolUse hooks (Bash for heartbeat/claude-check, AskUserQuestion for resume)
    let post_tool = hooks_obj.get("PostToolUse").unwrap().as_array().unwrap();
    assert_eq!(post_tool.len(), 2);
    // First hook: Bash matcher with post-tool-use.sh (heartbeat + claude attribution check)
    assert_eq!(post_tool[0]["matcher"], "Bash");
    assert!(post_tool[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("post-tool-use.sh"));
    // Second hook: AskUserQuestion matcher with ask-user-post.sh (stage resume)
    assert_eq!(post_tool[1]["matcher"], "AskUserQuestion");
    assert!(post_tool[1]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("ask-user-post.sh"));

    // Check Stop hook
    let stop = hooks_obj.get("Stop").unwrap().as_array().unwrap();
    assert_eq!(stop.len(), 1);
    assert!(stop[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("commit-guard.sh"));

    // Check UserPromptSubmit hook (skill suggestions)
    let user_prompt = hooks_obj
        .get("UserPromptSubmit")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(user_prompt.len(), 1);
    assert_eq!(user_prompt[0]["matcher"], "*");
    assert!(user_prompt[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("skill-trigger.sh"));
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
fn test_embedded_commit_guard_hook_is_valid() {
    // Verify the embedded hook script has correct shebang and key functions
    assert!(HOOK_COMMIT_GUARD.starts_with("#!/usr/bin/env bash"));
    // Check for key functions that should exist
    assert!(HOOK_COMMIT_GUARD.contains("detect_loom_worktree"));
    assert!(HOOK_COMMIT_GUARD.contains("check_git_clean"));
    assert!(HOOK_COMMIT_GUARD.contains("get_stage_status"));
    assert!(HOOK_COMMIT_GUARD.contains("block_with_reason"));
}

#[test]
fn test_install_loom_hooks_creates_hook_files() {
    // This test modifies ~/.claude/hooks/ which is a real directory
    // We'll verify the hooks are installed correctly
    let result = install_loom_hooks();
    assert!(result.is_ok());

    let home_dir = dirs::home_dir().expect("should have home dir");

    // Check global hooks are installed
    let commit_guard_path = home_dir.join(".claude/hooks/commit-guard.sh");
    if commit_guard_path.exists() {
        let content = fs::read_to_string(&commit_guard_path).unwrap();
        assert!(content.contains("detect_loom_worktree"));
        assert!(content.contains("LOOM WORKTREE EXIT BLOCKED"));
    }

    // Check worktree hooks are installed to loom/ subdirectory
    let worktree_hooks_dir = home_dir.join(".claude/hooks/loom");
    if worktree_hooks_dir.exists() {
        assert!(worktree_hooks_dir.join("post-tool-use.sh").exists());
        assert!(worktree_hooks_dir.join("session-start.sh").exists());
        assert!(worktree_hooks_dir.join("learning-validator.sh").exists());
    }
}

// ============================================================================
// sync_worktree_permissions tests
// ============================================================================

#[test]
fn test_sync_basic_permissions() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Create worktree settings with some permissions
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)", "Write(tests/**)"],
            "deny": ["Bash(rm -rf:*)"]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    // Run sync
    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // Verify permissions were synced
    assert_eq!(result.allow_added, 2);
    assert_eq!(result.deny_added, 1);

    // Read main settings and verify content
    let main_settings_path = main_dir.path().join(".claude/settings.local.json");
    let content = fs::read_to_string(&main_settings_path).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
    assert!(allow.iter().any(|v| v == "Write(tests/**)"));

    let deny = main_settings["permissions"]["deny"].as_array().unwrap();
    assert!(deny.iter().any(|v| v == "Bash(rm -rf:*)"));
}

#[test]
fn test_sync_filters_parent_traversal() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Create worktree settings with both regular and worktree-specific permissions
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": [
                "Read(src/**)",
                "Read(../../.work/**)",
                "Write(.worktrees/stage-1/**)",
                "Bash(cargo:*)"
            ]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    // Run sync
    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // Only non-worktree-specific permissions should be synced
    assert_eq!(result.allow_added, 2);

    // Verify main settings don't contain worktree-specific paths
    let main_settings_path = main_dir.path().join(".claude/settings.local.json");
    let content = fs::read_to_string(&main_settings_path).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
    assert!(allow.iter().any(|v| v == "Bash(cargo:*)"));
    assert!(!allow.iter().any(|v| v.as_str().unwrap().contains("../../")));
    assert!(!allow
        .iter()
        .any(|v| v.as_str().unwrap().contains(".worktrees/")));
}

#[test]
fn test_sync_deduplicates() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Create main settings with some existing permissions
    let main_claude_dir = main_dir.path().join(".claude");
    fs::create_dir_all(&main_claude_dir).unwrap();

    let main_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)"]
        }
    });
    fs::write(
        main_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&main_settings).unwrap(),
    )
    .unwrap();

    // Create worktree settings with overlapping and new permissions
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)", "Write(tests/**)"]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    // Run sync
    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // Only new permission should be added
    assert_eq!(result.allow_added, 1);

    // Verify main settings have both but no duplicates
    let content = fs::read_to_string(main_claude_dir.join("settings.local.json")).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    let read_count = allow.iter().filter(|v| *v == "Read(src/**)").count();
    assert_eq!(read_count, 1, "Read(src/**) should appear exactly once");
    assert!(allow.iter().any(|v| v == "Write(tests/**)"));
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

#[test]
fn test_sync_creates_main_settings() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Create worktree settings
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)"]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    // Verify main settings don't exist yet
    let main_settings_path = main_dir.path().join(".claude/settings.local.json");
    assert!(!main_settings_path.exists());

    // Run sync
    let result = sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    assert_eq!(result.allow_added, 1);

    // Verify main settings now exist and have the permission
    assert!(main_settings_path.exists());
    let content = fs::read_to_string(&main_settings_path).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
}

#[test]
fn test_sync_preserves_other_fields() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Create main settings with other fields
    let main_claude_dir = main_dir.path().join(".claude");
    fs::create_dir_all(&main_claude_dir).unwrap();

    let main_settings = json!({
        "permissions": {
            "allow": ["Read(existing/**)"]
        },
        "hooks": {
            "PreToolUse": []
        },
        "custom_field": "preserved",
        "nested": {
            "key": "value"
        }
    });
    fs::write(
        main_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&main_settings).unwrap(),
    )
    .unwrap();

    // Create worktree settings with new permissions
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)"]
        }
    });
    fs::write(
        worktree_claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&worktree_settings).unwrap(),
    )
    .unwrap();

    // Run sync
    sync_worktree_permissions(worktree_dir.path(), main_dir.path()).unwrap();

    // Verify other fields are preserved
    let content = fs::read_to_string(main_claude_dir.join("settings.local.json")).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    assert_eq!(main_settings["custom_field"], "preserved");
    assert_eq!(main_settings["nested"]["key"], "value");
    assert!(main_settings["hooks"]["PreToolUse"].as_array().is_some());

    // Verify permissions also include both old and new
    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(existing/**)"));
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
}

#[test]
fn test_sync_idempotent() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Create worktree settings
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": ["Read(src/**)", "Write(tests/**)"],
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
    assert_eq!(result1.deny_added, 1);

    // Second sync should add nothing (idempotent)
    assert_eq!(result2.allow_added, 0);
    assert_eq!(result2.deny_added, 0);

    // Verify final state has no duplicates
    let main_settings_path = main_dir.path().join(".claude/settings.local.json");
    let content = fs::read_to_string(&main_settings_path).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    assert_eq!(allow.len(), 2);

    let deny = main_settings["permissions"]["deny"].as_array().unwrap();
    assert_eq!(deny.len(), 1);
}
