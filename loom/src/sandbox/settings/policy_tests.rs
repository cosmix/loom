//! The codex grants of a stage session follow the configured backend: the
//! legacy lane gets its write paths, hosts and a readable `auth.json`; the
//! native backend gets none of them.

use serde_json::{json, Value};

use super::super::{build_settings_for_backend, SettingsTarget};
use crate::codex::{CodexBackend, CODEX_SANDBOX_DOMAINS, CODEX_SANDBOX_WRITE_PATHS};
use crate::fs::permissions::state_root::CODEX_AUTH_DENY_READ_PATH;
use crate::models::stage::{Implementer, Implementers};
use crate::plan::schema::{
    CommandConfinement, FilesystemConfig, LinuxConfig, NetworkConfig, PermissionMode,
};
use crate::sandbox::MergedSandboxConfig;

const ALLOW_WRITE: &str = "/sandbox/filesystem/allowWrite";
const DOMAINS: &str = "/sandbox/network/allowedDomains";
const DENY_READ: &str = "/sandbox/filesystem/denyRead";

fn config(lanes: Vec<Implementer>) -> MergedSandboxConfig {
    MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::new(lanes),
        command_confinement: CommandConfinement::default(),
    }
}

fn settings(config: &MergedSandboxConfig, backend: CodexBackend) -> Value {
    let existing = json!({});
    let target = SettingsTarget {
        is_worktree: true,
        state_root: None,
        existing: &existing,
        carry_plugin_keys: false,
        cwd: None,
    };
    build_settings_for_backend(config, &target, backend).expect("session settings")
}

/// The string array at `pointer` of `settings`; empty when there is none.
fn strings<'a>(settings: &'a Value, pointer: &str) -> Vec<&'a str> {
    settings
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[test]
fn native_backend_drops_write_paths_domains_and_keeps_auth_denied() {
    let codex = config(vec![Implementer::Codex, Implementer::Claude]);
    let native = settings(&codex, CodexBackend::Native);
    let legacy = settings(&codex, CodexBackend::Legacy);

    for path in CODEX_SANDBOX_WRITE_PATHS.into_iter().chain(["~/.codex"]) {
        let native_write = strings(&native, ALLOW_WRITE);
        assert!(
            !native_write.contains(&path),
            "Native allowWrite has {path}"
        );
        let legacy_write = strings(&legacy, ALLOW_WRITE);
        assert!(
            legacy_write.contains(&path),
            "Legacy allowWrite lacks {path}"
        );
    }
    for domain in CODEX_SANDBOX_DOMAINS.into_iter().chain(["api.openai.com"]) {
        assert!(
            !strings(&native, DOMAINS).contains(&domain),
            "Native has {domain}"
        );
        assert!(
            strings(&legacy, DOMAINS).contains(&domain),
            "Legacy lacks {domain}"
        );
    }
    assert!(strings(&native, DENY_READ).contains(&CODEX_AUTH_DENY_READ_PATH));
    assert!(!strings(&legacy, DENY_READ).contains(&CODEX_AUTH_DENY_READ_PATH));
}

#[test]
fn a_stage_without_codex_is_unchanged_by_the_backend() {
    let claude = config(vec![Implementer::Claude]);
    let native = settings(&claude, CodexBackend::Native);
    assert_eq!(native, settings(&claude, CodexBackend::Legacy));
    assert!(strings(&native, DENY_READ).contains(&CODEX_AUTH_DENY_READ_PATH));
    for domain in CODEX_SANDBOX_DOMAINS {
        assert!(!strings(&native, DOMAINS).contains(&domain));
    }
}
