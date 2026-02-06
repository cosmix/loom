//! Plan YAML schema type definitions

use serde::{Deserialize, Serialize};

/// Plan-level sandbox configuration (defaults for all stages)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Whether sandboxing is enabled (default: true)
    #[serde(default = "default_sandbox_enabled")]
    pub enabled: bool,

    /// Automatically allow sandbox permissions when stage starts (default: true)
    #[serde(default = "default_auto_allow")]
    pub auto_allow: bool,

    /// Allow agents to escape sandbox with explicit commands (default: false)
    #[serde(default)]
    pub allow_unsandboxed_escape: bool,

    /// Commands excluded from sandboxing (e.g., "loom" CLI)
    #[serde(default = "default_excluded_commands")]
    pub excluded_commands: Vec<String>,

    /// Filesystem access restrictions
    #[serde(default)]
    pub filesystem: FilesystemConfig,

    /// Network access restrictions
    #[serde(default)]
    pub network: NetworkConfig,

    /// Linux-specific configuration
    #[serde(default)]
    pub linux: LinuxConfig,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: default_sandbox_enabled(),
            auto_allow: default_auto_allow(),
            allow_unsandboxed_escape: false,
            excluded_commands: default_excluded_commands(),
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        }
    }
}

/// Per-stage sandbox configuration (overrides plan-level defaults)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
}

/// Filesystem access configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    /// Paths that agents cannot read (glob patterns)
    /// Default: ~/.ssh/**, ~/.aws/**, ~/.config/gcloud/**, ~/.gnupg/**
    #[serde(default = "default_deny_read")]
    pub deny_read: Vec<String>,

    /// Paths that agents cannot write (glob patterns)
    /// Default: .work/stages/**, doc/loom/knowledge/**
    #[serde(default = "default_deny_write")]
    pub deny_write: Vec<String>,

    /// Additional paths agents are allowed to write (glob patterns)
    /// Use this to grant exceptions to deny rules
    #[serde(default)]
    pub allow_write: Vec<String>,
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            deny_read: default_deny_read(),
            deny_write: default_deny_write(),
            allow_write: Vec::new(),
        }
    }
}

/// Network access configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Allowed network domains (glob patterns)
    /// Empty means no network access allowed
    #[serde(default)]
    pub allowed_domains: Vec<String>,

    /// Additional domains to allow beyond the defaults
    #[serde(default)]
    pub additional_domains: Vec<String>,

    /// Allow binding to local ports (default: false)
    #[serde(default)]
    pub allow_local_binding: bool,

    /// Allow Unix socket connections (default: false)
    #[serde(default)]
    pub allow_unix_sockets: bool,
}

/// Linux-specific sandbox configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinuxConfig {
    /// Enable weaker nested sandboxing for compatibility (default: false)
    /// Use this if running inside containers or VMs with restricted capabilities
    #[serde(default)]
    pub enable_weaker_nested: bool,
}

// Default value functions for serde
fn default_sandbox_enabled() -> bool {
    true
}

fn default_auto_allow() -> bool {
    true
}

fn default_excluded_commands() -> Vec<String> {
    vec!["loom".to_string()]
}

fn default_deny_read() -> Vec<String> {
    vec![
        // Sensitive credential directories
        "~/.ssh/**".to_string(),
        "~/.aws/**".to_string(),
        "~/.config/gcloud/**".to_string(),
        "~/.gnupg/**".to_string(),
        // Worktree escape prevention - block access to parent directories
        "../../**".to_string(),
        // Block access to other worktrees
        "../.worktrees/**".to_string(),
    ]
}

fn default_deny_write() -> Vec<String> {
    vec![
        // Worktree escape prevention - block writes to parent directories
        "../../**".to_string(),
        // Orchestration state files - managed by loom CLI only
        ".work/stages/**".to_string(),
        ".work/sessions/**".to_string(),
        // Knowledge files - protected by default, knowledge stages get explicit allow
        "doc/loom/knowledge/**".to_string(),
    ]
}

/// Type of stage for specialized handling.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::StageType.
pub use crate::models::stage::StageType;

/// Execution mode hint.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::ExecutionMode.
pub use crate::models::stage::ExecutionMode;

/// Root structure of the loom metadata block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoomMetadata {
    pub loom: LoomConfig,
}

/// Main loom configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoomConfig {
    pub version: u32,
    #[serde(default)]
    pub auto_merge: Option<bool>,
    /// Plan-level sandbox configuration (defaults for all stages)
    #[serde(default)]
    pub sandbox: SandboxConfig,
    /// Plan-level change impact configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_impact: Option<ChangeImpactConfig>,
    pub stages: Vec<StageDefinition>,
}

/// Stage definition from plan metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub parallel_group: Option<String>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub auto_merge: Option<bool>,
    /// Working directory for acceptance criteria, relative to worktree root.
    /// REQUIRED field - forces explicit choice of execution directory.
    /// Use "." for worktree root, or a subdirectory like "loom".
    pub working_dir: String,
    /// Type of stage for specialized handling (e.g., knowledge vs standard)
    #[serde(default)]
    pub stage_type: StageType,
    /// Observable behaviors that must be true from user perspective
    /// Each truth is a shell command that returns exit code 0 if the behavior works
    #[serde(default)]
    pub truths: Vec<String>,
    /// Files that must exist with real implementation (not stubs)
    /// Supports glob patterns like "src/auth/*.rs"
    #[serde(default)]
    pub artifacts: Vec<String>,
    /// Critical connections between components
    #[serde(default)]
    pub wiring: Vec<WiringCheck>,
    /// Enhanced truth checks with extended success criteria
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truth_checks: Vec<TruthCheck>,
    /// Runtime wiring tests (command-based integration verification)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiring_tests: Vec<WiringTest>,
    /// Dead code detection configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_code_check: Option<DeadCodeCheck>,
    /// Context budget as percentage (1-100). Default is 65%.
    /// When context usage exceeds this, auto-handoff is triggered.
    #[serde(default)]
    pub context_budget: Option<u32>,
    /// Per-stage sandbox configuration (overrides plan-level defaults)
    #[serde(default)]
    pub sandbox: StageSandboxConfig,
    /// Hint for execution mode (single agent vs team)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_mode: Option<ExecutionMode>,
}

/// Wiring check to verify component connections.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::WiringCheck.
pub use crate::models::stage::WiringCheck;

/// Enhanced truth check with extended success criteria.
///
/// TruthCheck allows verifying observable behaviors with more than just exit code.
/// All extended fields are optional for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthCheck {
    /// Shell command to execute
    pub command: String,
    /// Strings that must appear in stdout
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stdout_contains: Vec<String>,
    /// Strings that must NOT appear in stdout
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stdout_not_contains: Vec<String>,
    /// Whether stderr must be empty (default: false, meaning stderr is ignored)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_empty: Option<bool>,
    /// Expected exit code (default: 0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Human-readable description of what this truth verifies
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Success criteria for wiring tests.
///
/// Defines how to determine if a wiring test passed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuccessCriteria {
    /// Expected exit code (default: 0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Strings that must appear in stdout
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stdout_contains: Vec<String>,
    /// Strings that must NOT appear in stdout
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stdout_not_contains: Vec<String>,
    /// Strings that must appear in stderr
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stderr_contains: Vec<String>,
    /// Whether stderr must be empty
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_empty: Option<bool>,
}

/// Wiring test to verify component integration.
///
/// Unlike WiringCheck (grep-based pattern matching), WiringTest runs
/// actual commands to verify runtime behavior of component connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WiringTest {
    /// Human-readable name for this test
    pub name: String,
    /// Shell command to execute
    pub command: String,
    /// Success criteria for this test
    #[serde(default)]
    pub success_criteria: SuccessCriteria,
    /// Human-readable description of what this test verifies
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Configuration for dead code detection.
///
/// Runs a command and checks output for patterns indicating dead code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeCheck {
    /// Command to run for dead code detection (e.g., "cargo build --message-format=json")
    pub command: String,
    /// Patterns in output that indicate dead code (e.g., "warning: unused")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fail_patterns: Vec<String>,
    /// Patterns to ignore (e.g., "allowed_unused_function")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore_patterns: Vec<String>,
}

/// Policy for handling change impact failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeImpactPolicy {
    /// Fail the stage if impact check fails
    #[default]
    Fail,
    /// Warn but allow stage to continue
    Warn,
    /// Skip impact checking entirely
    Skip,
}

/// Configuration for change impact analysis.
///
/// Compares before/after states to detect unintended changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeImpactConfig {
    /// Command to generate baseline (run before changes)
    pub baseline_command: String,
    /// Command to generate comparison (run after changes)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare_command: Option<String>,
    /// Patterns in diff output that indicate failure
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failure_patterns: Vec<String>,
    /// Policy for handling failures
    #[serde(default)]
    pub policy: ChangeImpactPolicy,
}

/// Validation error with context
#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
    pub stage_id: Option<String>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(id) = &self.stage_id {
            write!(f, "Stage '{}': {}", id, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ValidationError {}
