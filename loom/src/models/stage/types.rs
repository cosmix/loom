use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::failure::FailureInfo;

/// Type of stage for specialized handling.
///
/// Re-exported from plan schema for convenience. Use this to distinguish
/// between knowledge-gathering stages and standard implementation stages.
pub use crate::plan::schema::StageType;

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
    pub acceptance: Vec<String>,
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
/// dependency work. See also [`crate::plan::graph::NodeStatus`].
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
        }
    }
}
