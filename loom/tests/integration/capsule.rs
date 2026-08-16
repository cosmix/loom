//! Integration coverage for the deterministic session capsule.
//!
//! NOTE ON SCOPE: `SessionCapsule`, `session_capsule` and `build_claude_command`
//! are `pub(crate)`, and this is an external test crate, so the capsule's argv
//! construction is unit-tested in-crate at
//! `loom/src/orchestrator/terminal/native/tests.rs` instead. What IS observable
//! from out here is the capsule's subject: the generated settings file it pins
//! with `--settings`. The capsule only narrows `--setting-sources` to
//! `user,project` when that file exists, so these tests pin the properties
//! the capsule depends on being true of it.

use std::fs;

use loom::models::stage::{Implementers, StageType};
use loom::plan::schema::{SandboxConfig, StageSandboxConfig};
use loom::sandbox::{merge_config, write_settings};
use serde_json::Value;
use tempfile::TempDir;

fn generated_settings() -> (TempDir, Value) {
    let target = tempfile::tempdir().unwrap();
    let config = merge_config(
        &SandboxConfig::default(),
        &StageSandboxConfig::default(),
        StageType::Standard,
        &Implementers::default(),
    );
    let settings_path = target.path().join(".claude/settings.local.json");

    write_settings(&config, target.path()).unwrap();
    let settings = serde_json::from_str(&fs::read_to_string(settings_path).unwrap()).unwrap();

    (target, settings)
}

#[test]
fn write_settings_creates_the_file_the_capsule_pins() {
    let (target, settings) = generated_settings();
    let settings_path = target.path().join(".claude/settings.local.json");

    assert!(settings_path.exists());
    assert!(settings.is_object());
}

#[test]
fn generated_settings_declare_the_sandbox_block() {
    let (_target, settings) = generated_settings();
    let sandbox = settings
        .get("sandbox")
        .and_then(Value::as_object)
        .expect("generated settings should contain a sandbox object");

    assert!(sandbox.contains_key("enabled"));
}

#[test]
fn generated_settings_use_edit_rules_not_write_rules() {
    // Claude Code's file permission check consults ONLY `Edit(path)` rules. A
    // `Write(path)` rule parses, prints a warning that scrolls past at session
    // startup, and is then ignored — so a generated `Write(...)` deny permits
    // exactly what it was written to block, and a `Write(...)` allow grants
    // nothing. Any regression back to that form is a silently inert policy,
    // which is the whole failure mode this file guards against.
    let (_target, settings) = generated_settings();
    let permissions = settings
        .get("permissions")
        .and_then(Value::as_object)
        .expect("generated settings should contain permissions");

    for section in ["allow", "deny"] {
        let entries = permissions
            .get(section)
            .and_then(Value::as_array)
            .expect("generated permissions should contain allow and deny arrays");
        assert!(entries.iter().all(|entry| {
            entry
                .as_str()
                .is_none_or(|rule| !rule.starts_with("Write("))
        }));
    }
}

#[test]
fn generated_settings_network_declares_a_strict_allowlist() {
    let (_target, settings) = generated_settings();
    let sandbox = settings
        .get("sandbox")
        .and_then(Value::as_object)
        .expect("generated settings should contain a sandbox object");

    assert_eq!(sandbox["network"]["strictAllowlist"], Value::Bool(true));
}
