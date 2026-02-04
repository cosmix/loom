//! Tests for sync_worktree_permissions

use crate::fs::permissions::sync::sync_worktree_permissions;
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

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
fn test_sync_transforms_worktree_paths() {
    let worktree_dir = TempDir::new().unwrap();
    let main_dir = TempDir::new().unwrap();

    // Create worktree settings with regular, transformable, and non-transformable permissions
    let worktree_claude_dir = worktree_dir.path().join(".claude");
    fs::create_dir_all(&worktree_claude_dir).unwrap();

    let worktree_settings = json!({
        "permissions": {
            "allow": [
                "Read(src/**)",                     // regular - keep as-is
                "Read(../../.work/**)",             // transformable - becomes Read(.work/**)
                "Write(.worktrees/stage-1/**)",    // non-transformable - filtered out
                "Bash(cargo:*)"                     // regular - keep as-is
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

    // Regular permissions + transformed permission should be synced
    // (Read(src/**), Read(.work/**), Bash(cargo:*))
    // Write(.worktrees/stage-1/**) is filtered out as non-transformable
    assert_eq!(result.allow_added, 3);

    // Verify main settings have correct permissions
    let main_settings_path = main_dir.path().join(".claude/settings.local.json");
    let content = fs::read_to_string(&main_settings_path).unwrap();
    let main_settings: Value = serde_json::from_str(&content).unwrap();

    let allow = main_settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));
    assert!(allow.iter().any(|v| v == "Bash(cargo:*)"));
    // Verify ../../.work/** was transformed to .work/**
    assert!(allow.iter().any(|v| v == "Read(.work/**)"));
    // Verify no raw worktree-specific patterns remain
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
