//! Security policy helpers for Claude settings generation.

use crate::sandbox::MergedSandboxConfig;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Credential files that must be denied even when an older plan omitted them.
const MANDATORY_DENY_READ: &[&str] = &["~/.claude/.credentials.json"];

/// Reject policy that cannot be represented without a sandbox escape.
pub(super) fn validate_emittable(config: &MergedSandboxConfig) -> Result<()> {
    if !config.excluded_commands.is_empty() {
        bail!(
            "sandbox.excluded_commands is not supported: command-prefix exclusions run outside \
             the host sandbox and cannot safely authorize extensible executors, VCS, build tools, \
             package managers, or the loom CLI"
        );
    }

    if config.enabled && !deny_read_patterns(config).is_empty() && !host_supports_deny_read() {
        bail!(
            "sandbox.filesystem.deny_read cannot be enforced on this host platform; refusing to \
             generate settings that claim an unenforceable read boundary"
        );
    }

    Ok(())
}

/// Effective OS-level read denials. Parent traversal is handled by worktree
/// confinement; emitting it here can resolve against the user's home directory.
pub(super) fn deny_read_patterns(config: &MergedSandboxConfig) -> Vec<String> {
    let mut seen = HashSet::new();
    config
        .filesystem
        .deny_read
        .iter()
        .map(String::as_str)
        .chain(MANDATORY_DENY_READ.iter().copied())
        .map(str::trim)
        .filter(|path| !path.is_empty() && !path.contains("../"))
        .filter(|path| seen.insert((*path).to_string()))
        .map(str::to_string)
        .collect()
}

/// Build the host sandbox portion of Claude's settings.
pub(super) fn sandbox_settings(config: &MergedSandboxConfig) -> Value {
    let mut sandbox = json!({ "enabled": config.enabled });
    if config.enabled {
        sandbox["failIfUnavailable"] = json!(true);
    }
    if config.auto_allow {
        sandbox["autoAllowBashIfSandboxed"] = json!(true);
    }
    if config.allow_unsandboxed_escape {
        sandbox["allowUnsandboxedCommands"] = json!(true);
    }
    if let Some(network) = network_settings(config) {
        sandbox["network"] = network;
    }
    if let Some(filesystem) = filesystem_settings(config) {
        sandbox["filesystem"] = filesystem;
    }
    if config.linux.enable_weaker_nested {
        sandbox["enableWeakerNestedSandbox"] = json!(true);
    }
    sandbox
}

fn network_settings(config: &MergedSandboxConfig) -> Option<Value> {
    let mut network = json!({});
    let mut domains = config.network.allowed_domains.clone();
    domains.extend(config.network.additional_domains.clone());
    if !domains.is_empty() {
        network["allowedDomains"] = json!(domains);
    }
    if config.network.allow_local_binding {
        network["allowLocalBinding"] = json!(true);
    }
    if !config.network.allow_unix_sockets.is_empty() {
        network["allowUnixSockets"] = json!(config.network.allow_unix_sockets);
    }
    if config.network.allow_all_unix_sockets {
        network["allowAllUnixSockets"] = json!(true);
    }
    network
        .as_object()
        .is_some_and(|value| !value.is_empty())
        .then_some(network)
}

fn filesystem_settings(config: &MergedSandboxConfig) -> Option<Value> {
    let mut filesystem = json!({});
    let deny_read = deny_read_patterns(config);
    if !deny_read.is_empty() {
        filesystem["denyRead"] = json!(deny_read);
    }
    let deny_write: Vec<&str> = config
        .filesystem
        .deny_write
        .iter()
        .filter(|path| !path.contains("../") && !path.starts_with("doc/loom/knowledge"))
        .map(String::as_str)
        .collect();
    if !deny_write.is_empty() {
        filesystem["denyWrite"] = json!(deny_write);
    }
    filesystem
        .as_object()
        .is_some_and(|value| !value.is_empty())
        .then_some(filesystem)
}

const fn host_supports_deny_read() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{FilesystemConfig, LinuxConfig, NetworkConfig, PermissionMode};

    fn config() -> MergedSandboxConfig {
        MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: Vec::new(),
            filesystem: FilesystemConfig {
                deny_read: vec!["~/.ssh/**".to_string(), "../../**".to_string()],
                deny_write: Vec::new(),
                allow_write: Vec::new(),
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: PermissionMode::Auto,
        }
    }

    #[test]
    fn adds_mandatory_credentials_and_filters_parent_traversal() {
        let paths = deny_read_patterns(&config());
        assert_eq!(paths, vec!["~/.ssh/**", "~/.claude/.credentials.json"]);
    }

    #[test]
    fn rejects_every_command_exclusion() {
        let mut config = config();
        config.excluded_commands = vec!["git:*".to_string()];
        let error = validate_emittable(&config).unwrap_err().to_string();
        assert!(error.contains("excluded_commands"));
        assert!(error.contains("outside the host sandbox"));
    }
}
