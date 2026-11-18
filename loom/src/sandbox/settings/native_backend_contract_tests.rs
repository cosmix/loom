//! Contract of stage native-ingress: with the native Codex backend a stage
//! session's sandbox settings carry none of the legacy Codex lane's grants.
//!
//! Call shape: `build_settings_for_backend(config, target, backend)`, the
//! backend-aware form of `build_settings`.

use serde_json::{json, Value};

use super::{build_settings_for_backend, SettingsTarget};
use crate::codex::{CodexBackend, CODEX_SANDBOX_DOMAINS, CODEX_SANDBOX_WRITE_PATHS};
use crate::models::stage::{Implementer, Implementers};
use crate::plan::schema::PermissionMode;
use crate::plan::schema::{CommandConfinement, FilesystemConfig, LinuxConfig, NetworkConfig};
use crate::sandbox::MergedSandboxConfig;

const AUTH_JSON: &str = "~/.codex/auth.json";

fn codex_and_claude_config() -> MergedSandboxConfig {
    MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::new(vec![Implementer::Codex, Implementer::Claude]),
        command_confinement: CommandConfinement::default(),
    }
}

/// The `sandbox` block of the session settings for `backend`, as string lists:
/// (allowWrite, allowedDomains, denyRead).
fn grants(backend: CodexBackend) -> (Vec<String>, Vec<String>, Vec<String>) {
    let existing = json!({});
    let target = SettingsTarget {
        is_worktree: true,
        state_root: None,
        existing: &existing,
        carry_plugin_keys: false,
        cwd: None,
    };
    let settings = build_settings_for_backend(&codex_and_claude_config(), &target, backend)
        .expect("session settings");
    let list = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let sandbox = &settings["sandbox"];
    (
        list(&sandbox["filesystem"]["allowWrite"]),
        list(&sandbox["network"]["allowedDomains"]),
        list(&sandbox["filesystem"]["denyRead"]),
    )
}

#[test]
fn native_backend_drops_session_codex_grants() {
    let (write, domains, deny_read) = grants(CodexBackend::Native);
    for path in CODEX_SANDBOX_WRITE_PATHS {
        assert!(
            !write.iter().any(|p| p == path),
            "Native allowWrite has {path}: {write:?}"
        );
    }
    for domain in CODEX_SANDBOX_DOMAINS {
        assert!(
            !domains.iter().any(|d| d == domain),
            "Native allowedDomains has {domain}: {domains:?}"
        );
    }
    assert!(!write.iter().any(|p| p == "~/.codex"), "{write:?}");
    assert!(
        !domains.iter().any(|d| d == "api.openai.com"),
        "{domains:?}"
    );
    assert!(
        deny_read.iter().any(|p| p == AUTH_JSON),
        "Native denyRead must keep {AUTH_JSON}: {deny_read:?}"
    );

    let (write, domains, deny_read) = grants(CodexBackend::Legacy);
    assert!(
        write.iter().any(|p| p == "~/.codex"),
        "Legacy allowWrite: {write:?}"
    );
    assert!(
        domains.iter().any(|d| d == "api.openai.com"),
        "Legacy allowedDomains: {domains:?}"
    );
    assert!(
        !deny_read.iter().any(|p| p == AUTH_JSON),
        "Legacy codex lane must read {AUTH_JSON}: {deny_read:?}"
    );
}
