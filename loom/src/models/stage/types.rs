use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::failure::FailureInfo;
use crate::plan::schema::{CodeReviewConfig, ContractSpec, ReachableCheck};

use super::checks::{default_plan_version, AcceptanceCriterion, TruthCheck, WiringCheck};
use super::dispute_budgets::DisputeTally;
use super::merge_block::MergeRecord;
use super::persisted::deserialize_persisted_reasoning_effort;
use super::sandbox_config::StageSandboxConfig;

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

/// A single agent lane a stage may delegate implementation work to.
///
/// Serialized as kebab-case in YAML (`claude`, `codex`). A closed enum rather
/// than a validated string: serde rejects unknown variants on its own, and the
/// closed set gives compile-time exhaustiveness at every match site.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Implementer {
    /// The Claude subagent lane (haiku for mechanical edits, boilerplate and simple unit tests,
    /// sonnet for common implementation and integration tests, opus for architecture and
    /// algorithm implementation (xhigh effort for the hardest), fable only for visual/UI
    /// design, a bug that survived a delegated fix attempt, or very hard algorithmic design).
    #[default]
    Claude,
    /// The codex implementation lane: spawned as `loom-codex-forwarder`, a
    /// loom-owned shim over the codex plugin's companion runtime (never the
    /// plugin's `codex:codex-rescue` directly).
    Codex,
}

impl std::fmt::Display for Implementer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Implementer::Claude => "claude",
            Implementer::Codex => "codex",
        };
        write!(f, "{s}")
    }
}

/// The set of agent lanes a stage is licensed to spawn subagents from.
///
/// A stage mixes lanes freely: routine implementation may go to codex while
/// tests go to sonnet and an architectural call stays with opus. Order is
/// meaningful — the FIRST lane is the one to reach for on routine
/// implementation ([`Implementers::preferred`]) — but every listed lane is
/// available for the parts of the stage that call for it.
///
/// Serialized transparently as a YAML sequence (`["codex", "claude"]`), so a
/// stage declares lanes the same way it declares any other list. An omitted
/// key defaults to `["claude"]`: the Claude lane is the harness the session
/// already runs in, so it needs no opt-in. Codex does — it needs a plugin, and
/// it needs the safety doctrine that [`Implementers::includes_codex`] gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Implementers(Vec<Implementer>);

impl Default for Implementers {
    fn default() -> Self {
        Self(vec![Implementer::Claude])
    }
}

impl Implementers {
    /// Build a lane set from an ordered list.
    pub fn new(lanes: Vec<Implementer>) -> Self {
        Self(lanes)
    }

    /// The lane to reach for on ROUTINE implementation work — the first listed.
    ///
    /// Falls back to [`Implementer::Claude`] for an empty set. Validation
    /// rejects an explicitly empty list, so the fallback only covers a value
    /// constructed in code rather than parsed from a plan.
    pub fn preferred(&self) -> Implementer {
        self.0.first().copied().unwrap_or_default()
    }

    /// Whether the codex lane is licensed for this stage.
    ///
    /// This is the gate for the codex safety doctrine: any stage that may
    /// spawn even ONE codex subagent has to carry the blast-radius rules,
    /// whether or not codex is its preferred lane.
    pub fn includes_codex(&self) -> bool {
        self.0.contains(&Implementer::Codex)
    }

    /// Whether the Claude subagent lane is licensed for this stage.
    pub fn includes_claude(&self) -> bool {
        self.0.contains(&Implementer::Claude)
    }

    /// Whether more than one lane is licensed.
    pub fn is_mixed(&self) -> bool {
        self.0.len() > 1
    }

    /// True when no lane is listed at all — an invalid state that validation rejects.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a Implementers {
    type Item = &'a Implementer;
    type IntoIter = std::slice::Iter<'a, Implementer>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl std::fmt::Display for Implementers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered: Vec<String> = self.0.iter().map(|l| l.to_string()).collect();
        write!(f, "{}", rendered.join(", "))
    }
}

/// Success criteria for wiring tests.
///
/// Defines how to determine if a wiring test passed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
    /// Human-facing summary of what the stage delivers, shown by the dashboard.
    #[serde(default)]
    pub summary: Option<String>,
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
    /// Why the post-merge worktree/branch cleanup failed or was refused, when
    /// it did. Set by `MergeLifecycle::cleanup`, cleared by the next cleanup
    /// that succeeds (including `loom worktree remove`). Surfaced by
    /// `loom status`; a merged stage with this set still has its worktree
    /// and branch on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_warning: Option<String>,
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
    /// Whether `merged` was asserted by an operator through
    /// `loom stage complete --force-unsafe --assume-merged` rather than set
    /// by a merge loom performed. Written on every `--force-unsafe`
    /// completion, so a later one without the flag resets it with `merged`;
    /// merges loom performs never touch it.
    #[serde(default)]
    pub merge_assumed: bool,
    /// Whether stage has unresolved merge conflicts
    #[serde(default)]
    pub merge_conflict: bool,
    /// Why the merge was not advanced and what it did with stashed changes;
    /// see [`MergeRecord`].
    #[serde(flatten)]
    pub merge: MergeRecord,
    /// Goal-backward verification status
    #[serde(default)]
    pub verification_status: VerificationStatus,
    /// Stage-specific context ceiling in tokens.
    #[serde(default)]
    pub context_ceiling_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_overview: Option<bool>,
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
    /// Structured review requirements for integration verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_review: Option<CodeReviewConfig>,
    /// Number of fix attempts made for this stage (acceptance/review cycles)
    #[serde(default)]
    pub fix_attempts: u32,
    /// Number of disputes filed against this stage's acceptance criteria.
    #[serde(default)]
    pub dispute_count: u32,
    /// Adjudication counters; see [`DisputeTally`].
    #[serde(flatten)]
    pub tally: DisputeTally,
    /// Times the daemon has recovered this stage from a stalled session — a
    /// live agent whose heartbeat went silent far past its response budget.
    /// Bounds that recovery so a stage that stalls every attempt is handed to
    /// an operator instead of being re-queued forever. Persisted rather than
    /// held in memory so a restarted daemon cannot reset the bound and resume
    /// the loop. Owned by `event_handler::recover_hung`.
    #[serde(default)]
    pub stall_recoveries: u32,
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
    /// Model override for this stage (e.g., "opus", "sonnet")
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
    /// License this stage's session for ultracode Workflow orchestration
    /// (multi-agent fan-out). Copied from the plan's StageDefinition.
    #[serde(default)]
    pub ultracode: bool,
    /// Which agent lanes this stage may spawn subagents from, in preference
    /// order. Copied from the plan's StageDefinition.
    #[serde(default)]
    pub implementers: Implementers,
    /// How long (seconds) this stage's session may go without a heartbeat before
    /// the orchestrator flags it as silent. Copied from the plan's StageDefinition;
    /// `None` means the built-in default. Resolve it through
    /// [`Stage::effective_subagent_timeout_secs`] rather than reading it directly.
    #[serde(default)]
    pub subagent_timeout_secs: Option<u64>,
    /// Skills this stage's agents need, by catalog name. Copied from the plan's StageDefinition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// The plan's `loom.version`; every v2 behaviour is gated on `plan_version == 2`.
    /// A stage file written before the field existed reads as 1.
    #[serde(default = "default_plan_version")]
    pub plan_version: u32,
    /// Behavioural contracts. Copied from the plan's StageDefinition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contracts: Vec<ContractSpec>,
    /// Globs the contract session may also edit. Copied from the plan's StageDefinition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub harness: Vec<String>,
    /// Reachability checks. Copied from the plan's StageDefinition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reachable: Vec<ReachableCheck>,
    /// Files changed only through an accepted integrity review. Copied from `loom.ratchet_files`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ratchet_files: Vec<String>,
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

    /// Stage's criterion was disputed (acceptance criterion, wiring
    /// check, or wiring test); awaiting an adjudicator verdict.
    /// The dispute records live at `.loom/work/disputes/<stage>/<n>/`.
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
