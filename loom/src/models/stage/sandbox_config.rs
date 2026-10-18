//! Sandbox configuration of a stage: command confinement and the filesystem,
//! network and Linux overrides a stage may set.

use serde::{Deserialize, Deserializer, Serialize};

use super::types::PermissionMode;

/// How far loom confines a plan-authored command when it executes it.
///
/// Plan YAML is a trusted artifact, but it is not daemon-authority code:
/// acceptance criteria, setup commands, truth checks, wiring tests and
/// dead-code checks all run as child processes of loom itself.
/// `Confined` (the default) rebuilds a minimal environment for those children
/// instead of handing them the daemon's ambient one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandConfinement {
    /// Minimal, allowlisted child environment (default).
    #[default]
    Confined,
    /// Inherit loom's own ambient environment. Explicit plan opt-in only.
    Inherit,
}

/// Per-stage sandbox configuration (overrides plan-level defaults)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageSandboxConfig {
    /// Override enabled setting for this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Override auto_allow setting for this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_allow: Option<bool>,

    /// Override allow_unsandboxed_escape for this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_unsandboxed_escape: Option<bool>,

    /// Additional excluded commands for this stage
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_commands: Vec<String>,

    /// Filesystem overrides for this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<FilesystemConfig>,

    /// Network overrides for this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkConfig>,

    /// Linux-specific overrides for this stage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<LinuxConfig>,

    /// Per-stage Claude Code permission-mode override.
    /// When unset, the plan-level override (or stage type default) applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,

    /// Per-stage override for how plan-authored commands are confined.
    /// When unset, the plan-level value applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_confinement: Option<CommandConfinement>,
}

/// Filesystem access configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemConfig {
    /// Paths that agents cannot read (glob patterns)
    /// Default: ~/.ssh/**, ~/.aws/**, ~/.config/gcloud/**, ~/.gnupg/**
    #[serde(default = "default_deny_read")]
    pub deny_read: Vec<String>,

    /// Paths that agents cannot write (glob patterns)
    /// Default: ../../**
    #[serde(default = "default_deny_write")]
    pub deny_write: Vec<String>,

    /// Additional paths agents are allowed to write (glob patterns) as exceptions to deny rules
    #[serde(default)]
    pub allow_write: Vec<String>,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            deny_read: default_deny_read(),
            deny_write: default_deny_write(),
            allow_write: vec![],
        }
    }
}

/// Network access configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkConfig {
    /// Allowed network domains (glob patterns); empty means no network access allowed
    #[serde(default)]
    pub allowed_domains: Vec<String>,

    /// Additional domains to allow beyond the defaults
    #[serde(default)]
    pub additional_domains: Vec<String>,

    /// Allow binding to local ports (default: false)
    #[serde(default)]
    pub allow_local_binding: bool,

    /// Allow specific Unix socket paths (glob patterns)
    /// Accepts either a list of paths or `false` (treated as empty list)
    #[serde(default, deserialize_with = "deserialize_bool_or_string_vec")]
    pub allow_unix_sockets: Vec<String>,

    /// Allow all Unix socket connections (default: false)
    #[serde(default)]
    pub allow_all_unix_sockets: bool,
}

/// Linux-specific sandbox configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinuxConfig {
    /// Enable weaker nested sandboxing for compatibility (default: false)
    /// Use this if running inside containers or VMs with restricted capabilities
    #[serde(default)]
    pub enable_weaker_nested: bool,
}

/// Deserializes a field that can be either a boolean `false` (→ empty vec) or a list of strings.
/// This allows plan authors to write `allow_unix_sockets: false` as shorthand for an empty list.
fn deserialize_bool_or_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrVec {
        Bool(bool),
        Vec(Vec<String>),
    }

    match BoolOrVec::deserialize(deserializer)? {
        BoolOrVec::Bool(false) => Ok(Vec::new()),
        BoolOrVec::Bool(true) => Err(serde::de::Error::custom(
            "allow_unix_sockets: true is ambiguous; use allow_all_unix_sockets: true to allow all sockets, or provide an explicit path list",
        )),
        BoolOrVec::Vec(v) => Ok(v),
    }
}

fn default_deny_read() -> Vec<String> {
    // Credential paths come from `state_root::CREDENTIAL_DENY_READ_PATHS`.
    let mut paths: Vec<String> = crate::fs::permissions::state_root::CREDENTIAL_DENY_READ_PATHS
        .iter()
        .map(|path| (*path).to_string())
        .collect();
    // State-root secrets reach only the OS denyRead list; the native file tools are
    // covered by credential-guard.sh. Include both layouts from either working directory.
    paths.extend(
        [".loom/work/", "../.loom/work/", ".work/", "../.work/"]
            .into_iter()
            .flat_map(|prefix| {
                crate::fs::permissions::state_root::STATE_ROOT_SECRET_FILES
                    .iter()
                    .map(move |name| format!("{prefix}{name}"))
            }),
    );
    paths.extend(["../../**", "../.worktrees/**"].map(String::from));
    paths
}

fn default_deny_write() -> Vec<String> {
    // Worktree escape prevention - block writes to parent directories.
    //
    // The knowledge directory is deliberately NOT denied here: every stage
    // records knowledge through the `loom knowledge update` CLI (a Bash
    // subprocess), and that subprocess runs *inside* the sandbox now that
    // `sandbox.excluded_commands` is rejected outright
    // (`sandbox/settings/policy.rs::validate_emittable`) — there is no
    // "outside the sandbox" escape hatch left for it to use. Denying the
    // path here would deny the CLI too, not just file tools, bricking
    // knowledge recording for every stage. Write access is instead
    // explicitly GRANTED via `sandbox::config::apply_knowledge_write_grant`,
    // and the file-tool-only "use the CLI, not Edit/Write" doctrine is
    // enforced by `loom-hooks/worktree-file-guard.sh`, which can block file tools
    // without blocking the CLI subprocess.
    vec!["../../**".to_string()]
}
