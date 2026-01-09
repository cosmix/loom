use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub close_reason: Option<String>,
}

/// Status of a stage in the execution lifecycle.
///
/// State machine transitions:
/// - `WaitingForDeps` → `Queued` (when all dependencies complete)
/// - `Queued` → `Executing` (when session spawns)
/// - `Executing` → `Completed` | `Blocked` | `NeedsHandoff` | `WaitingForInput`
/// - `WaitingForInput` → `Executing` (when input provided)
/// - `Blocked` → `Queued` (when unblocked)
/// - `NeedsHandoff` → `Queued` (when new session resumes)
/// - `Completed` is a terminal state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageStatus {
    /// Stage is waiting for upstream dependencies to complete.
    /// Cannot be executed until all dependencies reach Completed status.
    #[serde(rename = "waiting-for-deps", alias = "pending")]
    WaitingForDeps,

    /// Stage dependencies are satisfied; queued for execution.
    /// Orchestrator will pick from Queued stages to spawn sessions.
    #[serde(rename = "queued", alias = "ready")]
    Queued,

    /// Stage is actively being worked on by a Claude session.
    #[serde(rename = "executing")]
    Executing,

    /// Stage needs user input/decision before continuing.
    #[serde(rename = "waiting-for-input")]
    WaitingForInput,

    /// Stage encountered an error and was stopped.
    /// Can be unblocked back to Queued after intervention.
    #[serde(rename = "blocked")]
    Blocked,

    /// Stage work is done; terminal state.
    #[serde(rename = "completed", alias = "verified")]
    Completed,

    /// Session hit context limit; needs new session to continue.
    #[serde(rename = "needs-handoff", alias = "needshandoff")]
    NeedsHandoff,
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
        }
    }
}

impl StageStatus {
    /// Check if transitioning from the current status to the new status is valid.
    ///
    /// Valid transitions:
    /// - `WaitingForDeps` -> `Queued` (when dependencies satisfied)
    /// - `Queued` -> `Executing` (when session spawned)
    /// - `Executing` -> `Completed` | `Blocked` | `NeedsHandoff` | `WaitingForInput`
    /// - `Blocked` -> `Queued` (when unblocked)
    /// - `NeedsHandoff` -> `Queued` (when resumed)
    /// - `WaitingForInput` -> `Executing` (when input provided)
    /// - `Completed` is a terminal state
    ///
    /// # Arguments
    /// * `new_status` - The target status to transition to
    ///
    /// # Returns
    /// `true` if the transition is valid, `false` otherwise
    pub fn can_transition_to(&self, new_status: &StageStatus) -> bool {
        // Same status is always valid (no-op)
        if self == new_status {
            return true;
        }

        match self {
            StageStatus::WaitingForDeps => matches!(new_status, StageStatus::Queued),
            StageStatus::Queued => matches!(new_status, StageStatus::Executing),
            StageStatus::Executing => matches!(
                new_status,
                StageStatus::Completed
                    | StageStatus::Blocked
                    | StageStatus::NeedsHandoff
                    | StageStatus::WaitingForInput
            ),
            StageStatus::WaitingForInput => matches!(new_status, StageStatus::Executing),
            StageStatus::Completed => false, // Terminal state
            StageStatus::Blocked => matches!(new_status, StageStatus::Queued),
            StageStatus::NeedsHandoff => matches!(new_status, StageStatus::Queued),
        }
    }

    /// Attempt to transition to a new status, returning an error if invalid.
    ///
    /// # Arguments
    /// * `new_status` - The target status to transition to
    ///
    /// # Returns
    /// `Ok(new_status)` if the transition is valid, `Err` otherwise
    pub fn try_transition(&self, new_status: StageStatus) -> Result<StageStatus> {
        if self.can_transition_to(&new_status) {
            Ok(new_status)
        } else {
            bail!("Invalid stage status transition: {self} -> {new_status}")
        }
    }

    /// Returns the list of valid statuses this status can transition to.
    pub fn valid_transitions(&self) -> Vec<StageStatus> {
        match self {
            StageStatus::WaitingForDeps => vec![StageStatus::Queued],
            StageStatus::Queued => vec![StageStatus::Executing],
            StageStatus::Executing => vec![
                StageStatus::Completed,
                StageStatus::Blocked,
                StageStatus::NeedsHandoff,
                StageStatus::WaitingForInput,
            ],
            StageStatus::WaitingForInput => vec![StageStatus::Executing],
            StageStatus::Completed => vec![], // Terminal state
            StageStatus::Blocked => vec![StageStatus::Queued],
            StageStatus::NeedsHandoff => vec![StageStatus::Queued],
        }
    }
}

impl Stage {
    pub fn new(name: String, description: Option<String>) -> Self {
        let now = Utc::now();
        let id = Self::generate_id(&name);

        Self {
            id,
            name,
            description,
            status: StageStatus::WaitingForDeps,
            dependencies: Vec::new(),
            parallel_group: None,
            acceptance: Vec::new(),
            setup: Vec::new(),
            files: Vec::new(),
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: Vec::new(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            close_reason: None,
        }
    }

    pub fn generate_id(name: &str) -> String {
        let timestamp = Utc::now().timestamp();
        format!(
            "stage-{}-{}",
            name.to_lowercase().replace(' ', "-"),
            timestamp
        )
    }

    pub fn add_dependency(&mut self, stage_id: String) {
        if !self.dependencies.contains(&stage_id) {
            self.dependencies.push(stage_id);
            self.updated_at = Utc::now();
        }
    }

    pub fn remove_dependency(&mut self, stage_id: &str) {
        if let Some(pos) = self.dependencies.iter().position(|id| id == stage_id) {
            self.dependencies.remove(pos);
            self.updated_at = Utc::now();
        }
    }

    pub fn set_parallel_group(&mut self, group: Option<String>) {
        self.parallel_group = group;
        self.updated_at = Utc::now();
    }

    pub fn add_acceptance_criterion(&mut self, criterion: String) {
        self.acceptance.push(criterion);
        self.updated_at = Utc::now();
    }

    pub fn add_file_pattern(&mut self, pattern: String) {
        if !self.files.contains(&pattern) {
            self.files.push(pattern);
            self.updated_at = Utc::now();
        }
    }

    pub fn set_plan(&mut self, plan_id: String) {
        self.plan_id = Some(plan_id);
        self.updated_at = Utc::now();
    }

    pub fn set_worktree(&mut self, worktree_id: Option<String>) {
        self.worktree = worktree_id;
        self.updated_at = Utc::now();
    }

    pub fn assign_session(&mut self, session_id: String) {
        self.session = Some(session_id);
        self.updated_at = Utc::now();
    }

    pub fn release_session(&mut self) {
        self.session = None;
        self.updated_at = Utc::now();
    }

    pub fn add_child_stage(&mut self, child_id: String) {
        if !self.child_stages.contains(&child_id) {
            self.child_stages.push(child_id);
            self.updated_at = Utc::now();
        }
    }

    /// Attempt to transition the stage to a new status with validation.
    ///
    /// This is the primary method for changing stage status. It validates
    /// that the transition is allowed before applying it.
    ///
    /// # Arguments
    /// * `new_status` - The target status to transition to
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if the transition is invalid
    pub fn try_transition(&mut self, new_status: StageStatus) -> Result<()> {
        let validated_status = self.status.try_transition(new_status)?;
        self.status = validated_status;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Complete the stage with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_complete(&mut self, reason: Option<String>) -> Result<()> {
        self.try_transition(StageStatus::Completed)?;
        self.completed_at = Some(Utc::now());
        self.close_reason = reason;
        Ok(())
    }

    /// Mark the stage as needing handoff with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_needs_handoff(&mut self) -> Result<()> {
        self.try_transition(StageStatus::NeedsHandoff)
    }

    /// Mark the stage as queued with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_queued(&mut self) -> Result<()> {
        self.try_transition(StageStatus::Queued)
    }

    /// Mark the stage as executing with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_executing(&mut self) -> Result<()> {
        self.try_transition(StageStatus::Executing)
    }

    /// Mark the stage as waiting for input with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_waiting_for_input(&mut self) -> Result<()> {
        self.try_transition(StageStatus::WaitingForInput)
    }

    /// Mark the stage as blocked with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_blocked(&mut self) -> Result<()> {
        self.try_transition(StageStatus::Blocked)
    }

    // Deprecated: Use try_complete for validated transitions
    #[deprecated(since = "0.2.0", note = "Use try_complete for validated transitions")]
    pub fn complete(&mut self, reason: Option<String>) {
        self.status = StageStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.close_reason = reason;
        self.updated_at = Utc::now();
    }

    // Deprecated: Use try_mark_needs_handoff for validated transitions
    #[deprecated(
        since = "0.2.0",
        note = "Use try_mark_needs_handoff for validated transitions"
    )]
    pub fn mark_needs_handoff(&mut self) {
        self.status = StageStatus::NeedsHandoff;
        self.updated_at = Utc::now();
    }

    // Deprecated: Use try_mark_queued for validated transitions
    #[deprecated(
        since = "0.2.0",
        note = "Use try_mark_queued for validated transitions"
    )]
    pub fn mark_queued(&mut self) {
        self.status = StageStatus::Queued;
        self.updated_at = Utc::now();
    }

    // Deprecated: Use try_mark_executing for validated transitions
    #[deprecated(
        since = "0.2.0",
        note = "Use try_mark_executing for validated transitions"
    )]
    pub fn mark_executing(&mut self) {
        self.status = StageStatus::Executing;
        self.updated_at = Utc::now();
    }

    // Deprecated: Use try_mark_waiting_for_input for validated transitions
    #[deprecated(
        since = "0.2.0",
        note = "Use try_mark_waiting_for_input for validated transitions"
    )]
    pub fn mark_waiting_for_input(&mut self) {
        self.status = StageStatus::WaitingForInput;
        self.updated_at = Utc::now();
    }

    pub fn hold(&mut self) {
        if !self.held {
            self.held = true;
            self.updated_at = Utc::now();
        }
    }

    pub fn release(&mut self) {
        if self.held {
            self.held = false;
            self.updated_at = Utc::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_stage(status: StageStatus) -> Stage {
        let mut stage = Stage::new(
            "Test Stage".to_string(),
            Some("Test description".to_string()),
        );
        stage.status = status;
        stage
    }

    // =========================================================================
    // StageStatus::can_transition_to tests
    // =========================================================================

    #[test]
    fn test_waiting_for_deps_can_transition_to_queued() {
        let status = StageStatus::WaitingForDeps;
        assert!(status.can_transition_to(&StageStatus::Queued));
    }

    #[test]
    fn test_waiting_for_deps_cannot_transition_to_other_states() {
        let status = StageStatus::WaitingForDeps;
        assert!(!status.can_transition_to(&StageStatus::Executing));
        assert!(!status.can_transition_to(&StageStatus::Completed));
        assert!(!status.can_transition_to(&StageStatus::Blocked));
        assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
        assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
    }

    #[test]
    fn test_queued_can_transition_to_executing() {
        let status = StageStatus::Queued;
        assert!(status.can_transition_to(&StageStatus::Executing));
    }

    #[test]
    fn test_queued_cannot_transition_to_other_states() {
        let status = StageStatus::Queued;
        assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
        assert!(!status.can_transition_to(&StageStatus::Completed));
        assert!(!status.can_transition_to(&StageStatus::Blocked));
        assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
    }

    #[test]
    fn test_executing_can_transition_to_valid_states() {
        let status = StageStatus::Executing;
        assert!(status.can_transition_to(&StageStatus::Completed));
        assert!(status.can_transition_to(&StageStatus::Blocked));
        assert!(status.can_transition_to(&StageStatus::NeedsHandoff));
        assert!(status.can_transition_to(&StageStatus::WaitingForInput));
    }

    #[test]
    fn test_executing_cannot_transition_to_invalid_states() {
        let status = StageStatus::Executing;
        assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
        assert!(!status.can_transition_to(&StageStatus::Queued));
    }

    #[test]
    fn test_waiting_for_input_can_transition_to_executing() {
        let status = StageStatus::WaitingForInput;
        assert!(status.can_transition_to(&StageStatus::Executing));
    }

    #[test]
    fn test_waiting_for_input_cannot_transition_to_other_states() {
        let status = StageStatus::WaitingForInput;
        assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
        assert!(!status.can_transition_to(&StageStatus::Queued));
        assert!(!status.can_transition_to(&StageStatus::Completed));
    }

    #[test]
    fn test_completed_is_terminal_state() {
        let status = StageStatus::Completed;
        assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
        assert!(!status.can_transition_to(&StageStatus::Queued));
        assert!(!status.can_transition_to(&StageStatus::Executing));
        assert!(!status.can_transition_to(&StageStatus::Blocked));
        assert!(!status.can_transition_to(&StageStatus::NeedsHandoff));
        assert!(!status.can_transition_to(&StageStatus::WaitingForInput));
    }

    #[test]
    fn test_blocked_can_transition_to_queued() {
        let status = StageStatus::Blocked;
        assert!(status.can_transition_to(&StageStatus::Queued));
    }

    #[test]
    fn test_blocked_cannot_transition_to_other_states() {
        let status = StageStatus::Blocked;
        assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
        assert!(!status.can_transition_to(&StageStatus::Executing));
        assert!(!status.can_transition_to(&StageStatus::Completed));
    }

    #[test]
    fn test_needs_handoff_can_transition_to_queued() {
        let status = StageStatus::NeedsHandoff;
        assert!(status.can_transition_to(&StageStatus::Queued));
    }

    #[test]
    fn test_needs_handoff_cannot_transition_to_other_states() {
        let status = StageStatus::NeedsHandoff;
        assert!(!status.can_transition_to(&StageStatus::WaitingForDeps));
        assert!(!status.can_transition_to(&StageStatus::Executing));
        assert!(!status.can_transition_to(&StageStatus::Completed));
    }

    #[test]
    fn test_same_status_transition_is_valid() {
        let statuses = vec![
            StageStatus::WaitingForDeps,
            StageStatus::Queued,
            StageStatus::Executing,
            StageStatus::Completed,
            StageStatus::Blocked,
            StageStatus::NeedsHandoff,
            StageStatus::WaitingForInput,
        ];

        for status in statuses {
            assert!(
                status.can_transition_to(&status.clone()),
                "Same-state transition should be valid for {status:?}"
            );
        }
    }

    // =========================================================================
    // StageStatus::try_transition tests
    // =========================================================================

    #[test]
    fn test_try_transition_valid_waiting_for_deps_to_queued() {
        let status = StageStatus::WaitingForDeps;
        let result = status.try_transition(StageStatus::Queued);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), StageStatus::Queued);
    }

    #[test]
    fn test_try_transition_invalid_completed_to_waiting_for_deps() {
        let status = StageStatus::Completed;
        let result = status.try_transition(StageStatus::WaitingForDeps);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid stage status transition"));
        assert!(err.contains("Completed"));
        assert!(err.contains("WaitingForDeps"));
    }

    // =========================================================================
    // StageStatus::valid_transitions tests
    // =========================================================================

    #[test]
    fn test_valid_transitions_waiting_for_deps() {
        let transitions = StageStatus::WaitingForDeps.valid_transitions();
        assert_eq!(transitions, vec![StageStatus::Queued]);
    }

    #[test]
    fn test_valid_transitions_executing() {
        let transitions = StageStatus::Executing.valid_transitions();
        assert_eq!(transitions.len(), 4);
        assert!(transitions.contains(&StageStatus::Completed));
        assert!(transitions.contains(&StageStatus::Blocked));
        assert!(transitions.contains(&StageStatus::NeedsHandoff));
        assert!(transitions.contains(&StageStatus::WaitingForInput));
    }

    #[test]
    fn test_valid_transitions_completed() {
        let transitions = StageStatus::Completed.valid_transitions();
        assert!(transitions.is_empty());
    }

    // =========================================================================
    // Stage::try_transition tests
    // =========================================================================

    #[test]
    fn test_stage_try_transition_valid() {
        let mut stage = create_test_stage(StageStatus::WaitingForDeps);
        let result = stage.try_transition(StageStatus::Queued);
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::Queued);
    }

    #[test]
    fn test_stage_try_transition_invalid() {
        let mut stage = create_test_stage(StageStatus::Completed);
        let result = stage.try_transition(StageStatus::WaitingForDeps);
        assert!(result.is_err());
        assert_eq!(stage.status, StageStatus::Completed); // Status unchanged
    }

    #[test]
    fn test_stage_try_mark_queued_from_pending() {
        let mut stage = create_test_stage(StageStatus::WaitingForDeps);
        let result = stage.try_mark_queued();
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::Queued);
    }

    #[test]
    fn test_stage_try_mark_queued_from_blocked() {
        let mut stage = create_test_stage(StageStatus::Blocked);
        let result = stage.try_mark_queued();
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::Queued);
    }

    #[test]
    fn test_stage_try_mark_queued_from_needs_handoff() {
        let mut stage = create_test_stage(StageStatus::NeedsHandoff);
        let result = stage.try_mark_queued();
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::Queued);
    }

    #[test]
    fn test_stage_try_mark_queued_invalid() {
        let mut stage = create_test_stage(StageStatus::Completed);
        let result = stage.try_mark_queued();
        assert!(result.is_err());
    }

    #[test]
    fn test_stage_try_mark_executing_valid() {
        let mut stage = create_test_stage(StageStatus::Queued);
        let result = stage.try_mark_executing();
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::Executing);
    }

    #[test]
    fn test_stage_try_complete_valid() {
        let mut stage = create_test_stage(StageStatus::Executing);
        let result = stage.try_complete(Some("Done".to_string()));
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(stage.completed_at.is_some());
        assert_eq!(stage.close_reason, Some("Done".to_string()));
    }

    #[test]
    fn test_stage_try_complete_invalid() {
        let mut stage = create_test_stage(StageStatus::WaitingForDeps);
        let result = stage.try_complete(None);
        assert!(result.is_err());
        assert_eq!(stage.status, StageStatus::WaitingForDeps);
    }

    #[test]
    fn test_stage_try_mark_blocked_valid() {
        let mut stage = create_test_stage(StageStatus::Executing);
        let result = stage.try_mark_blocked();
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::Blocked);
    }

    #[test]
    fn test_stage_try_mark_needs_handoff_valid() {
        let mut stage = create_test_stage(StageStatus::Executing);
        let result = stage.try_mark_needs_handoff();
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::NeedsHandoff);
    }

    #[test]
    fn test_stage_try_mark_waiting_for_input_valid() {
        let mut stage = create_test_stage(StageStatus::Executing);
        let result = stage.try_mark_waiting_for_input();
        assert!(result.is_ok());
        assert_eq!(stage.status, StageStatus::WaitingForInput);
    }

    // =========================================================================
    // Full workflow tests
    // =========================================================================

    #[test]
    fn test_full_happy_path_workflow() {
        let mut stage = create_test_stage(StageStatus::WaitingForDeps);

        // WaitingForDeps -> Queued
        assert!(stage.try_mark_queued().is_ok());
        assert_eq!(stage.status, StageStatus::Queued);

        // Queued -> Executing
        assert!(stage.try_mark_executing().is_ok());
        assert_eq!(stage.status, StageStatus::Executing);

        // Executing -> Completed (terminal state)
        assert!(stage.try_complete(None).is_ok());
        assert_eq!(stage.status, StageStatus::Completed);

        // Completed is terminal, no further transitions allowed
        assert!(stage.try_mark_queued().is_err());
    }

    #[test]
    fn test_blocked_recovery_workflow() {
        let mut stage = create_test_stage(StageStatus::Executing);

        // Executing -> Blocked
        assert!(stage.try_mark_blocked().is_ok());
        assert_eq!(stage.status, StageStatus::Blocked);

        // Blocked -> Queued (after unblocking)
        assert!(stage.try_mark_queued().is_ok());
        assert_eq!(stage.status, StageStatus::Queued);

        // Queued -> Executing (resume)
        assert!(stage.try_mark_executing().is_ok());
        assert_eq!(stage.status, StageStatus::Executing);
    }

    #[test]
    fn test_handoff_recovery_workflow() {
        let mut stage = create_test_stage(StageStatus::Executing);

        // Executing -> NeedsHandoff
        assert!(stage.try_mark_needs_handoff().is_ok());
        assert_eq!(stage.status, StageStatus::NeedsHandoff);

        // NeedsHandoff -> Queued (after new session picks up)
        assert!(stage.try_mark_queued().is_ok());
        assert_eq!(stage.status, StageStatus::Queued);
    }

    #[test]
    fn test_waiting_for_input_workflow() {
        let mut stage = create_test_stage(StageStatus::Executing);

        // Executing -> WaitingForInput
        assert!(stage.try_mark_waiting_for_input().is_ok());
        assert_eq!(stage.status, StageStatus::WaitingForInput);

        // WaitingForInput -> Executing (input provided)
        assert!(stage.try_mark_executing().is_ok());
        assert_eq!(stage.status, StageStatus::Executing);
    }

    #[test]
    fn test_display_implementation() {
        assert_eq!(format!("{}", StageStatus::WaitingForDeps), "WaitingForDeps");
        assert_eq!(format!("{}", StageStatus::Queued), "Queued");
        assert_eq!(format!("{}", StageStatus::Executing), "Executing");
        assert_eq!(
            format!("{}", StageStatus::WaitingForInput),
            "WaitingForInput"
        );
        assert_eq!(format!("{}", StageStatus::Blocked), "Blocked");
        assert_eq!(format!("{}", StageStatus::Completed), "Completed");
        assert_eq!(format!("{}", StageStatus::NeedsHandoff), "NeedsHandoff");
    }
}
