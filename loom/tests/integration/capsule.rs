//! Integration coverage for the deterministic session capsule.
//!
//! NOTE ON SCOPE: `SessionCapsule`, `session_capsule` and `build_claude_command`
//! are `pub(crate)`, and this is an external test crate, so the capsule's argv
//! construction is unit-tested in-crate at
//! `loom/src/orchestrator/terminal/native/tests.rs` instead. What IS observable
//! from out here is the capsule's subject: the generated settings document the
//! capsule pins with `--settings`. `write_settings` (the file-writing entry
//! point) is gone — the capsule and these tests both build the document
//! through the pure builder instead.

use loom::models::stage::{Implementers, StageType};
use loom::plan::schema::{SandboxConfig, StageSandboxConfig};
use loom::sandbox::{generate_settings_json, merge_config};
use serde_json::Value;

fn generated_settings() -> Value {
    let config = merge_config(
        &SandboxConfig::default(),
        &StageSandboxConfig::default(),
        StageType::Standard,
        &Implementers::default(),
    );
    generate_settings_json(&config)
}

#[test]
fn generated_settings_declare_the_sandbox_block() {
    let settings = generated_settings();
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
    let settings = generated_settings();
    let permissions = settings
        .get("permissions")
        .and_then(Value::as_object)
        .expect("generated settings should contain permissions");

    let empty = Vec::new();
    for section in ["allow", "deny"] {
        // The generator omits a section entirely when it would be empty, so
        // an absent section is equivalent to an empty array here, not a
        // failure to assert against.
        let entries = permissions
            .get(section)
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        assert!(entries.iter().all(|entry| {
            entry
                .as_str()
                .is_none_or(|rule| !rule.starts_with("Write("))
        }));
    }
}

#[test]
fn generated_settings_network_declares_a_strict_allowlist() {
    let settings = generated_settings();
    let sandbox = settings
        .get("sandbox")
        .and_then(Value::as_object)
        .expect("generated settings should contain a sandbox object");

    assert_eq!(sandbox["network"]["strictAllowlist"], Value::Bool(true));
}
