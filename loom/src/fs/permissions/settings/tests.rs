use super::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_ensure_loom_permissions_creates_settings_json_with_permissions_only() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    // settings.json should have permissions but NOT hooks or env
    let settings_path = repo_root.join(".claude/settings.json");
    assert!(settings_path.exists());

    let content = fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Permissions should be present
    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Bash(loom *)"));

    // Hooks should NOT be in settings.json
    assert!(settings.get("hooks").is_none());

    // Env should NOT be in settings.json
    assert!(settings.get("env").is_none());
}

#[test]
fn test_ensure_loom_permissions_creates_hooks_in_settings_local() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    // settings.local.json should have hooks and env
    let settings_local_path = repo_root.join(".claude/settings.local.json");
    assert!(settings_local_path.exists());

    let content = fs::read_to_string(&settings_local_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Hooks should be present
    assert!(settings.get("hooks").is_some());

    // Env should be present
    assert_eq!(settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"], "1");
}

#[test]
fn test_ensure_loom_disables_worktree_isolation_in_settings_local() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    let settings_local_path = repo_root.join(".claude/settings.local.json");
    let content = fs::read_to_string(&settings_local_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Worktree isolation must be off so main-repo subagents (knowledge
    // stages, interactive sessions) don't spawn nested worktrees.
    assert_eq!(settings["worktree"]["bgIsolation"], "none");

    // Running again is idempotent — the value is already "none".
    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();
    let content = fs::read_to_string(&settings_local_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(settings["worktree"]["bgIsolation"], "none");
}

#[test]
fn test_ensure_loom_permissions_preserves_existing_settings_local() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let hooks_dir = temp_dir.path().join("hooks");

    // Create .claude directory and pre-existing settings.local.json with sandbox config
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    let existing = json!({
        "permissions": {
            "allow": ["Read(src/**)"]
        },
        "sandbox": {
            "enabled": true
        }
    });
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions_to(repo_root, Some(&hooks_dir)).unwrap();

    // Read back settings.local.json
    let content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Existing sandbox config should be preserved
    assert_eq!(settings["sandbox"]["enabled"], true);

    // Existing permissions should be preserved
    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(src/**)"));

    // Hooks should be added
    assert!(settings.get("hooks").is_some());

    // Env should be added
    assert_eq!(settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"], "1");
}

#[test]
fn test_migrate_hooks_from_settings_json() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // Create .claude directory with settings.json that has old hooks + env
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    let old_settings = json!({
        "permissions": {
            "allow": ["Bash(loom *)"]
        },
        "hooks": {
            "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "/home/user/.claude/hooks/loom/commit-filter.sh"}]}]
        },
        "env": {
            "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&old_settings).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions_to(repo_root, Some(&temp_dir.path().join("hooks"))).unwrap();

    // settings.json should no longer have hooks or env
    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    assert!(settings.get("hooks").is_none());
    assert!(settings.get("env").is_none());

    // settings.local.json should have hooks and env
    let local_content = fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
    let local_settings: Value = serde_json::from_str(&local_content).unwrap();
    assert!(local_settings.get("hooks").is_some());
    assert_eq!(
        local_settings["env"]["CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"],
        "1"
    );
}

#[test]
fn test_migrate_removes_session_identity_from_settings_json() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // Very old loom versions persisted session identity in settings.json
    let old_settings = json!({
        "permissions": { "allow": ["Bash(loom *)"] },
        "env": {
            "LOOM_STAGE_ID": "knowledge-bootstrap",
            "LOOM_SESSION_ID": "session-stale",
            "LOOM_WORK_DIR": "/repo/.work"
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&old_settings).unwrap(),
    )
    .unwrap();

    ensure_loom_permissions_to(repo_root, Some(&temp_dir.path().join("hooks"))).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let env = settings["env"].as_object().unwrap();
    assert!(!env.contains_key("LOOM_STAGE_ID"));
    assert!(!env.contains_key("LOOM_SESSION_ID"));
    // Stable, repo-scoped value survives
    assert_eq!(env["LOOM_WORK_DIR"], "/repo/.work");
}

#[test]
fn test_scrub_main_repo_settings_identity_heals_both_files() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // A LIVE work dir: it must survive the identity heal alongside the
    // dead session-identity keys.
    let live_work_dir = repo_root.join(".work");
    fs::create_dir_all(&live_work_dir).unwrap();
    let live_work_dir_str = live_work_dir.to_string_lossy().to_string();

    let polluted = json!({
        "env": {
            "LOOM_STAGE_ID": "knowledge-bootstrap",
            "LOOM_SESSION_ID": "session-stale",
            "LOOM_MAIN_AGENT_PID": "12345",
            "LOOM_WORK_DIR": live_work_dir_str
        },
        "permissions": { "allow": ["Bash(loom *)"] }
    });
    for name in ["settings.json", "settings.local.json"] {
        fs::write(
            claude_dir.join(name),
            serde_json::to_string_pretty(&polluted).unwrap(),
        )
        .unwrap();
    }

    let healed = scrub_main_repo_settings_identity(repo_root);
    assert_eq!(healed.len(), 2);

    for name in ["settings.json", "settings.local.json"] {
        let content = fs::read_to_string(claude_dir.join(name)).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        let env = settings["env"].as_object().unwrap();
        assert!(!env.contains_key("LOOM_STAGE_ID"), "{name}");
        assert!(!env.contains_key("LOOM_SESSION_ID"), "{name}");
        assert!(!env.contains_key("LOOM_MAIN_AGENT_PID"), "{name}");
        assert_eq!(env["LOOM_WORK_DIR"], live_work_dir_str, "{name}");
        // Unrelated sections untouched
        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert!(allow.iter().any(|v| v == "Bash(loom *)"), "{name}");
    }
}

#[test]
fn test_scrub_main_repo_settings_identity_noop_cases() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // No .claude directory at all
    assert!(scrub_main_repo_settings_identity(repo_root).is_empty());

    // Clean file with a LIVE work dir → nothing healed, file byte-identical
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let live_work_dir = repo_root.join(".work");
    fs::create_dir_all(&live_work_dir).unwrap();
    let clean = serde_json::to_string_pretty(&json!({
        "env": { "LOOM_WORK_DIR": live_work_dir.to_string_lossy() }
    }))
    .unwrap();
    fs::write(claude_dir.join("settings.local.json"), &clean).unwrap();
    assert!(scrub_main_repo_settings_identity(repo_root).is_empty());
    assert_eq!(
        fs::read_to_string(claude_dir.join("settings.local.json")).unwrap(),
        clean
    );

    // Unparseable file is skipped without error
    fs::write(claude_dir.join("settings.json"), "{not json").unwrap();
    assert!(scrub_main_repo_settings_identity(repo_root).is_empty());
}

#[test]
fn test_settings_json_has_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // No hooks
    let settings = json!({"permissions": {"allow": []}});
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    assert!(!settings_json_has_hooks(repo_root));

    // With hooks
    let settings = json!({"hooks": {"PreToolUse": []}});
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    assert!(settings_json_has_hooks(repo_root));
}
