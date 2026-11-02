use super::*;

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

    // User hooks do not need migration.
    let settings = json!({"hooks": {"PreToolUse": [{"hooks":[{"command":"/custom/check.sh"}]}]}});
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    assert!(!settings_json_has_hooks(repo_root));

    let settings = json!({"hooks": {"PreToolUse": [{"hooks":[{"command":"~/.claude/hooks/commit-guard.sh"}]}]}});
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    assert!(settings_json_has_hooks(repo_root));
}
