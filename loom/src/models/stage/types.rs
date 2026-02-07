use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    /// Can use `loom memory` and `loom knowledge` (for promoting memories)
    IntegrationVerify,
    /// Code review stage (e.g., code-review)
    /// Reviews code, can use `loom memory` and `loom knowledge` commands
    /// Exempt from goal-backward verification requirements
    CodeReview,
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
    /// Observable behaviors that must work (shell commands return 0)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truths: Vec<String>,
    /// Files that must exist with real implementation (not stubs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    /// Critical connections between components
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wiring: Vec<WiringCheck>,
    /// Per-stage sandbox configuration
    #[serde(default)]
    pub sandbox: crate::plan::schema::StageSandboxConfig,
    /// Hint for execution mode (single agent vs team)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_mode: Option<ExecutionMode>,
    /// Number of fix attempts made for this stage (acceptance/review cycles)
    #[serde(default)]
    pub fix_attempts: u32,
    /// Maximum fix attempts allowed (None = use default of 3)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fix_attempts: Option<u32>,
    /// Reason the stage was flagged for human review
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_reason: Option<String>,
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
        }
    }

    /// Returns whether this status should be bold
    pub fn is_bold(&self) -> bool {
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
        };
        style = style.fg(color);

        if self.is_bold() {
            style = style.add_modifier(Modifier::BOLD);
        }

        style
    }

    /// Returns a short label for this status
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
            Self::MergeBlocked => "MergeErr",
            Self::NeedsHumanReview => "Review",
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
            truths: Vec::new(),
            artifacts: Vec::new(),
            wiring: Vec::new(),
            sandbox: Default::default(),
            execution_mode: None,
            fix_attempts: 0,
            max_fix_attempts: None,
            review_reason: None,
        }
    }
}
