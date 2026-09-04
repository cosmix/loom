//! Security policy helpers for Claude settings generation.

use crate::codex::{CODEX_SANDBOX_DOMAINS, CODEX_SANDBOX_WRITE_PATHS};
use crate::fs::permissions::state_root::CREDENTIAL_DENY_READ_PATHS;
use crate::sandbox::{MergedSandboxConfig, PACKAGE_MANAGER_CACHE_WRITE_PATHS};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Credential paths that must be denied even when an older plan omitted them.
/// The same list `models::stage::types::default_deny_read` supplies, so a plan
/// carrying the defaults adds nothing here — `deny_read_patterns` dedupes.
const MANDATORY_DENY_READ: &[&str] = &CREDENTIAL_DENY_READ_PATHS;

/// Reject policy that cannot be represented without a sandbox escape.
pub(crate) fn validate_emittable(config: &MergedSandboxConfig) -> Result<()> {
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
    // Emitting literal false makes the policy auditable in generated settings;
    // an absent key is indistinguishable from a key the reader has not looked for.
    sandbox["autoAllowBashIfSandboxed"] = json!(config.auto_allow);
    sandbox["allowUnsandboxedCommands"] = json!(config.allow_unsandboxed_escape);
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
    // The codex lane's own hosts follow the stage's licensed lanes, sparing a
    // licensed stage a mid-run domain decision it cannot answer.
    if config.implementers.includes_codex() {
        for domain in CODEX_SANDBOX_DOMAINS {
            if !domains.iter().any(|existing| existing == domain) {
                domains.push(domain.to_string());
            }
        }
    }
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
    if config.enabled {
        network["strictAllowlist"] = json!(true);
    }
    if config.enabled {
        return Some(network);
    }
    network
        .as_object()
        .is_some_and(|value| !value.is_empty())
        .then_some(network)
}

/// Push `path` onto `into` unless it is already present (dedup, keeps order).
fn push_unique(into: &mut Vec<String>, path: &str) {
    if !into.iter().any(|existing| existing == path) {
        into.push(path.to_string());
    }
}

fn filesystem_settings(config: &MergedSandboxConfig) -> Option<Value> {
    let mut filesystem = json!({});
    let deny_read = deny_read_patterns(config);
    if !deny_read.is_empty() {
        filesystem["denyRead"] = json!(deny_read);
    }
    // The `doc/loom/knowledge` filter is defense-in-depth, not the grant
    // itself: `merge_config`'s `apply_knowledge_write_grant`
    // (`sandbox/config.rs`) already strips the path from `deny_write` and
    // adds it to `allow_write` for every `MergedSandboxConfig` built the
    // normal way. This filter only guards a config assembled by hand that
    // bypassed that path — it grants nothing on its own.
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
    // `allowWrite` is the OS-enforced, additive write grant. Plan entries
    // reach it directly; package-manager caches are granted to every stage
    // (`sandbox::package_caches`); codex state directories only when that
    // lane is licensed.
    let mut allow_write: Vec<String> = Vec::new();
    for path in config
        .filesystem
        .allow_write
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && !p.contains("../"))
    {
        push_unique(&mut allow_write, path);
    }
    for path in PACKAGE_MANAGER_CACHE_WRITE_PATHS {
        push_unique(&mut allow_write, path);
    }
    if config.implementers.includes_codex() {
        for path in CODEX_SANDBOX_WRITE_PATHS {
            push_unique(&mut allow_write, path);
        }
    }
    if !allow_write.is_empty() {
        filesystem["allowWrite"] = json!(allow_write);
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
mod tests;
