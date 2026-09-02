//! Tests for settings functions

use crate::fs::permissions::settings::{
    ensure_loom_permissions_to, scrub_session_identity_env, scrub_stale_work_dir_env,
};
use serde_json::{json, Value};
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_ensure_loom_permissions_creates_new_file() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    let settings_path = repo_root.join(".claude/settings.json");
    assert!(settings_path.exists());

    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Bash(loom *)"));
}

#[test]
fn test_ensure_loom_permissions_merges_existing() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");
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

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Check existing permissions preserved
    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));

    // Check loom CLI permissions added
    assert!(allow.iter().any(|v| v == "Bash(loom *)"));

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
    let hooks_dir = temp_dir.path().join("hooks");
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // Create existing settings with some loom permissions already
    let existing = json!({
        "permissions": {
            "allow": ["Bash(loom *)"]
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let allow = settings["permissions"]["allow"].as_array().unwrap();

    // Count occurrences of Bash(loom *) - should be exactly 1
    let loom_count = allow.iter().filter(|v| *v == "Bash(loom *)").count();
    assert_eq!(loom_count, 1);
}

#[test]
fn test_ensure_loom_permissions_adds_hooks_to_settings_local() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    // Hooks should be in settings.local.json, NOT settings.json
    let settings_local_path = repo_root.join(".claude/settings.local.json");
    let content = fs::read_to_string(&settings_local_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Check hooks are configured in settings.local.json
    let hooks = settings.get("hooks").expect("hooks should be present");
    let hooks_obj = hooks.as_object().unwrap();

    assert!(hooks_obj.contains_key("PreToolUse"));
    assert!(hooks_obj.contains_key("PostToolUse"));
    assert!(hooks_obj.contains_key("Stop"));

    // settings.json should NOT have hooks
    let settings_path = repo_root.join(".claude/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    assert!(settings.get("hooks").is_none());
}

#[test]
fn test_hooks_not_duplicated_on_rerun() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    // Run twice
    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();
    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    // Hooks are in settings.local.json
    let settings_local_path = repo_root.join(".claude/settings.local.json");
    let content = fs::read_to_string(&settings_local_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Should still have exactly one Stop hook entry
    let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop_hooks.len(), 1);
}

#[test]
fn test_scrub_stale_work_dir_env_removes_dead_pin() {
    let temp_dir = TempDir::new().unwrap();
    // A path under a TempDir that was never created — guaranteed absent.
    let dead_path = temp_dir.path().join("gone").join(".loom").join("work");

    let mut settings = json!({
        "env": { "LOOM_WORK_DIR": dead_path.to_string_lossy() }
    });

    assert!(scrub_stale_work_dir_env(&mut settings));
    assert!(settings["env"].get("LOOM_WORK_DIR").is_none());
}

#[test]
fn test_scrub_stale_work_dir_env_preserves_live_pin() {
    let temp_dir = TempDir::new().unwrap();
    let live_path = temp_dir.path().join(".loom").join("work");
    fs::create_dir_all(&live_path).unwrap();

    let mut settings = json!({
        "env": { "LOOM_WORK_DIR": live_path.to_string_lossy() }
    });

    assert!(!scrub_stale_work_dir_env(&mut settings));
    assert_eq!(
        settings["env"]["LOOM_WORK_DIR"],
        json!(live_path.to_string_lossy())
    );
}

#[test]
fn test_scrub_stale_work_dir_env_noop_without_env_block() {
    let mut settings = json!({ "permissions": { "allow": ["Bash(loom *)"] } });

    assert!(!scrub_stale_work_dir_env(&mut settings));
    assert_eq!(
        settings,
        json!({ "permissions": { "allow": ["Bash(loom *)"] } })
    );
}

#[test]
fn test_scrub_stale_work_dir_env_noop_missing_key() {
    let mut settings = json!({ "env": { "FOO": "keep" } });

    assert!(!scrub_stale_work_dir_env(&mut settings));
    assert_eq!(settings["env"]["FOO"], json!("keep"));
}

#[test]
#[serial]
fn test_ensure_loom_permissions_adds_home_expanded_codex_forward_entry() {
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::home_dir_resolvable(),
        "fs::permissions::tests::settings_tests::test_ensure_loom_permissions_adds_home_expanded_codex_forward_entry",
        "the home directory is not resolvable in this sandbox",
    ) {
        return;
    }
    // Drives the real write path (not just the LOOM_PERMISSIONS constant) so
    // this would actually catch the fold site being missed. This is also the
    // exact file `create_worktree_settings` copies verbatim into every new
    // worktree's settings.json, so proving both spellings land here proves
    // the worktree context inherits both too — worktrees never independently
    // fold LOOM_PERMISSIONS_WORKTREE (nothing does; see concerns.md), they
    // get their allow-list from a straight copy of this file at creation.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    let settings_path = repo_root.join(".claude/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let allow = settings["permissions"]["allow"].as_array().unwrap();

    // The pre-existing `~` spelling must still be present (additive fix).
    assert!(allow
        .iter()
        .any(|v| v == "Bash(~/.claude/hooks/loom/codex-forward.sh:*)"));

    // The new home-expanded spelling must also be present.
    let home = dirs::home_dir().expect("test environment must have a resolvable home dir");
    let expected = format!(
        "Bash({}/.claude/hooks/loom/codex-forward.sh:*)",
        home.display()
    );
    assert!(
        allow.iter().any(|v| v == expected.as_str()),
        "expected {expected:?} in allow list, got {allow:?}"
    );
}

#[test]
#[serial]
fn test_ensure_loom_permissions_home_expanded_entry_no_duplicates_on_rerun() {
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::home_dir_resolvable(),
        "fs::permissions::tests::settings_tests::test_ensure_loom_permissions_home_expanded_entry_no_duplicates_on_rerun",
        "the home directory is not resolvable in this sandbox",
    ) {
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    // Run twice, as a repeated `loom init`/`loom repair --fix` would.
    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();
    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    let settings_path = repo_root.join(".claude/settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let allow = settings["permissions"]["allow"].as_array().unwrap();

    let home = dirs::home_dir().expect("test environment must have a resolvable home dir");
    let expected = format!(
        "Bash({}/.claude/hooks/loom/codex-forward.sh:*)",
        home.display()
    );
    let count = allow.iter().filter(|v| *v == expected.as_str()).count();
    assert_eq!(count, 1, "home-expanded entry must not be duplicated");
}

#[test]
fn test_scrub_identity_and_stale_work_dir_together() {
    let temp_dir = TempDir::new().unwrap();
    let dead_path = temp_dir.path().join("gone").join(".loom").join("work");

    let mut settings = json!({
        "env": {
            "LOOM_MAIN_AGENT_PID": "12345",
            "LOOM_STAGE_ID": "stale-stage",
            "LOOM_SESSION_ID": "stale-session",
            "LOOM_WORK_DIR": dead_path.to_string_lossy(),
            "FOO": "keep"
        }
    });

    let identity_removed = scrub_session_identity_env(&mut settings);
    let work_dir_removed = scrub_stale_work_dir_env(&mut settings);
    assert!(identity_removed);
    assert!(work_dir_removed);

    let env = settings["env"].as_object().unwrap();
    assert!(!env.contains_key("LOOM_MAIN_AGENT_PID"));
    assert!(!env.contains_key("LOOM_STAGE_ID"));
    assert!(!env.contains_key("LOOM_SESSION_ID"));
    assert!(!env.contains_key("LOOM_WORK_DIR"));
    assert_eq!(env["FOO"], json!("keep"));
}
