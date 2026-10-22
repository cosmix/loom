//! Contracts for the sandbox route closure: the credential files every session
//! capsule must read-deny, and the codex credential deny that a codex-licensed
//! stage lifts.

use loom::models::stage::{Implementer, Implementers, StageType};
use loom::plan::schema::{FilesystemConfig, SandboxConfig, StageSandboxConfig};
use loom::sandbox::{generate_settings_json, merge_config};
use serde_json::Value;

const CREDENTIAL_FILES: [&str; 9] = [
    "~/.config/gh/hosts.yml",
    "~/.netrc",
    "~/.npmrc",
    "~/.git-credentials",
    "~/.docker/config.json",
    "~/.kube/config",
    "~/.pypirc",
    "~/.cargo/credentials.toml",
    "~/.cargo/credentials",
];

const CODEX_AUTH: &str = "~/.codex/auth.json";

fn deny_read(plan: &SandboxConfig, implementers: &Implementers) -> Vec<String> {
    let merged = merge_config(
        plan,
        &StageSandboxConfig::default(),
        StageType::Standard,
        implementers,
    );
    let settings = generate_settings_json(&merged);
    settings
        .pointer("/sandbox/filesystem/denyRead")
        .and_then(Value::as_array)
        .expect("settings should carry /sandbox/filesystem/denyRead as an array")
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .expect("every denyRead entry should be a string")
                .to_string()
        })
        .collect()
}

#[test]
fn credential_files_are_read_denied_whatever_the_plan_lists() {
    let plan = SandboxConfig {
        filesystem: FilesystemConfig {
            deny_read: vec!["~/.ssh/**".to_string()],
            deny_write: vec![],
            allow_write: vec![],
        },
        ..SandboxConfig::default()
    };
    let denied = deny_read(&plan, &Implementers::default());
    let missing: Vec<&str> = CREDENTIAL_FILES
        .iter()
        .copied()
        .filter(|file| !denied.iter().any(|entry| entry == file))
        .collect();
    assert!(
        missing.is_empty(),
        "a plan listing its own deny_read must still read-deny every credential file; \
         missing {missing:?} from {denied:?}"
    );
}

#[test]
fn codex_auth_is_read_denied_unless_the_stage_licenses_codex() {
    let plan = SandboxConfig::default();
    let claude_only = deny_read(&plan, &Implementers::default());
    assert!(
        claude_only.iter().any(|entry| entry == CODEX_AUTH),
        "a stage without the codex lane must read-deny {CODEX_AUTH}; got {claude_only:?}"
    );
    let licensed = Implementers::new(vec![Implementer::Claude, Implementer::Codex]);
    let codex_lane = deny_read(&plan, &licensed);
    assert!(
        !codex_lane.iter().any(|entry| entry == CODEX_AUTH),
        "a codex-licensed stage must be able to read {CODEX_AUTH}; got {codex_lane:?}"
    );
}
