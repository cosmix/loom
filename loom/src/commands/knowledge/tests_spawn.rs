//! Tests for `commands/knowledge/spawn.rs`.

use super::*;

fn sandbox_json(allow_writes: bool) -> serde_json::Value {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");
    write_knowledge_sandbox(temp.path(), allow_writes).expect("Failed to write sandbox settings");
    let raw = std::fs::read_to_string(temp.path().join(".claude/settings.local.json"))
        .expect("Failed to read settings.local.json");
    serde_json::from_str(&raw).expect("Settings are not valid JSON")
}

fn rules(settings: &serde_json::Value, key: &str) -> Vec<String> {
    settings["permissions"][key]
        .as_array()
        .expect("permissions list missing")
        .iter()
        .map(|v| v.as_str().expect("rule is not a string").to_string())
        .collect()
}

#[test]
fn test_dry_run_denies_all_edits() {
    // Regression: dry-run used to deny `Write(**)`, which Claude Code parses but
    // never consults for file permissions — so "dry-run" could edit files.
    let settings = sandbox_json(false);
    assert!(rules(&settings, "deny").contains(&"Edit(**)".to_string()));
}

#[test]
fn test_dry_run_grants_no_edit_permission() {
    let settings = sandbox_json(false);
    let allow = rules(&settings, "allow");
    assert!(!allow.iter().any(|r| r.starts_with("Edit(")));
}

#[test]
fn test_write_mode_allows_editing_knowledge_dir() {
    let settings = sandbox_json(true);
    assert!(rules(&settings, "allow").contains(&"Edit(doc/loom/knowledge/**)".to_string()));
}

#[test]
fn test_write_mode_has_no_blanket_edit_deny() {
    // Deny beats allow, so a blanket `Edit(**)` deny in write mode would block
    // the knowledge directory the session exists to edit.
    let settings = sandbox_json(true);
    assert!(!rules(&settings, "deny").contains(&"Edit(**)".to_string()));
}

#[test]
fn test_no_write_rules_are_emitted() {
    // `Write(path)` rules are inert for file permission checks in Claude Code and
    // only produce startup warnings — every rule must be expressed as `Edit(path)`.
    for allow_writes in [true, false] {
        let settings = sandbox_json(allow_writes);
        for key in ["allow", "deny"] {
            assert!(
                !rules(&settings, key)
                    .iter()
                    .any(|r| r.starts_with("Write(")),
                "found a Write() rule in {key} (allow_writes={allow_writes})"
            );
        }
    }
}

#[test]
fn test_secret_reads_stay_denied_in_both_modes() {
    for allow_writes in [true, false] {
        let deny = rules(&sandbox_json(allow_writes), "deny");
        assert!(deny.contains(&"Read(~/.ssh/**)".to_string()));
        assert!(deny.contains(&"Read(~/.aws/**)".to_string()));
    }
}

#[test]
fn test_restore_removes_settings_when_there_was_no_backup() {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");
    let settings_path = temp.path().join(".claude/settings.local.json");

    write_knowledge_sandbox(temp.path(), false).expect("Failed to write sandbox settings");
    assert!(settings_path.exists());

    restore_sandbox_settings(temp.path(), None).expect("Failed to restore");
    assert!(!settings_path.exists());
}

#[test]
fn test_restore_puts_back_the_callers_settings() {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");
    let claude_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("Failed to create .claude dir");
    let settings_path = claude_dir.join("settings.local.json");
    let original = r#"{"permissions":{"allow":["Bash(cargo test)"]}}"#;
    std::fs::write(&settings_path, original).expect("Failed to seed settings");

    let backup = write_knowledge_sandbox(temp.path(), false).expect("Failed to write sandbox");
    assert_eq!(backup.as_deref(), Some(original));
    assert_ne!(
        std::fs::read_to_string(&settings_path).expect("Failed to read"),
        original
    );

    restore_sandbox_settings(temp.path(), backup).expect("Failed to restore");
    assert_eq!(
        std::fs::read_to_string(&settings_path).expect("Failed to read"),
        original
    );
}
