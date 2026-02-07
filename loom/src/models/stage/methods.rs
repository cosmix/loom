use anyhow::Result;
use chrono::{DateTime, Utc};

use super::types::{Stage, StageOutput, StageStatus, StageType};

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
            truth_checks: Vec::new(),
            wiring_tests: Vec::new(),
            dead_code_check: None,
            fix_attempts: 0,
            sandbox: Default::default(),
            execution_mode: None,
            max_fix_attempts: None,
            review_reason: None,
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

    pub fn set_worktree(&mut self, worktree_id: Option<String>) {
        self.worktree = worktree_id;
        self.updated_at = Utc::now();
    }

    pub fn set_resolved_base(&mut self, base: Option<String>) {
        self.resolved_base = base;
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
    /// Computes and stores `duration_secs` from `started_at` to completion.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_complete(&mut self, reason: Option<String>) -> Result<()> {
        self.try_transition(StageStatus::Completed)?;
        let now = Utc::now();
        self.completed_at = Some(now);
        self.close_reason = reason;
        // Compute duration from started_at to completed_at
        if let Some(start) = self.started_at {
            self.duration_secs = Some(now.signed_duration_since(start).num_seconds());
        }
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
    /// Sets `started_at` timestamp if not already set (preserves original
    /// start time across retries).
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_executing(&mut self) -> Result<()> {
        self.try_transition(StageStatus::Executing)?;
        // Only set started_at on first execution (preserve across retries)
        if self.started_at.is_none() {
            self.started_at = Some(Utc::now());
        }
        Ok(())
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

    /// Mark the stage as skipped with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_skip(&mut self, reason: Option<String>) -> Result<()> {
        self.try_transition(StageStatus::Skipped)?;
        self.close_reason = reason;
        Ok(())
    }

    /// Mark the stage as having merge conflicts.
    ///
    /// This sets both the status to MergeConflict and the merge_conflict flag.
    /// The stage work is complete but cannot be merged due to conflicts.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_merge_conflict(&mut self) -> Result<()> {
        self.try_transition(StageStatus::MergeConflict)?;
        self.merge_conflict = true;
        Ok(())
    }

    /// Complete merge conflict resolution and mark stage as completed.
    ///
    /// This clears the merge_conflict flag and marks the stage as merged.
    /// Computes and stores `duration_secs` from `started_at` to completion.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_complete_merge(&mut self) -> Result<()> {
        self.try_transition(StageStatus::Completed)?;
        self.merge_conflict = false;
        self.merged = true;
        let now = Utc::now();
        self.completed_at = Some(now);
        // Compute duration from started_at to completed_at
        if let Some(start) = self.started_at {
            self.duration_secs = Some(now.signed_duration_since(start).num_seconds());
        }
        Ok(())
    }

    /// Mark the stage as completed with failures (acceptance criteria failed).
    ///
    /// This indicates the stage finished executing but acceptance criteria failed.
    /// The stage can be retried by transitioning back to Executing.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_complete_with_failures(&mut self) -> Result<()> {
        self.try_transition(StageStatus::CompletedWithFailures)
    }

    /// Mark the stage as merge blocked (merge failed with actual error, not conflicts).
    ///
    /// This indicates the merge operation failed due to an error (not conflicts).
    /// The stage can be retried by transitioning back to Executing.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_merge_blocked(&mut self) -> Result<()> {
        self.try_transition(StageStatus::MergeBlocked)
    }

    /// Request human review for this stage.
    ///
    /// Transitions from Executing to NeedsHumanReview and records the reason.
    ///
    /// # Arguments
    /// * `reason` - Why the stage needs human review
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_request_human_review(&mut self, reason: String) -> Result<()> {
        self.try_transition(StageStatus::NeedsHumanReview)?;
        self.review_reason = Some(reason);
        Ok(())
    }

    /// Approve human review and resume execution.
    ///
    /// Transitions from NeedsHumanReview back to Executing.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_approve_review(&mut self) -> Result<()> {
        self.try_transition(StageStatus::Executing)?;
        self.review_reason = None;
        Ok(())
    }

    /// Force-complete a stage that is in human review.
    ///
    /// Transitions from NeedsHumanReview to Completed.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_force_complete_review(&mut self) -> Result<()> {
        self.try_transition(StageStatus::Completed)?;
        let now = Utc::now();
        self.completed_at = Some(now);
        if let Some(start) = self.started_at {
            self.duration_secs = Some(now.signed_duration_since(start).num_seconds());
        }
        Ok(())
    }

    /// Reject human review and block the stage.
    ///
    /// Transitions from NeedsHumanReview to Blocked and updates the review reason.
    ///
    /// # Arguments
    /// * `reason` - Why the review was rejected
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_reject_review(&mut self, reason: String) -> Result<()> {
        self.try_transition(StageStatus::Blocked)?;
        self.review_reason = Some(reason);
        Ok(())
    }

    /// Increment the fix attempt counter and return the new count.
    pub fn increment_fix_attempts(&mut self) -> u32 {
        self.fix_attempts += 1;
        self.updated_at = Utc::now();
        self.fix_attempts
    }

    /// Check if the stage has reached its fix attempt limit.
    pub fn is_at_fix_limit(&self) -> bool {
        self.fix_attempts >= self.get_effective_max_fix_attempts()
    }

    /// Get the effective maximum fix attempts (default 3 if not set).
    pub fn get_effective_max_fix_attempts(&self) -> u32 {
        self.max_fix_attempts.unwrap_or(3)
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

    /// Add or update an output for this stage.
    ///
    /// If an output with the same key already exists, it will be replaced.
    ///
    /// # Arguments
    /// * `output` - The output to add or update
    ///
    /// # Returns
    /// `true` if the output was added, `false` if it replaced an existing output
    pub fn set_output(&mut self, output: StageOutput) -> bool {
        self.updated_at = Utc::now();

        // Check if an output with this key already exists
        if let Some(existing) = self.outputs.iter_mut().find(|o| o.key == output.key) {
            *existing = output;
            false
        } else {
            self.outputs.push(output);
            true
        }
    }

    /// Get an output by key.
    ///
    /// # Arguments
    /// * `key` - The key of the output to retrieve
    ///
    /// # Returns
    /// The output if found, None otherwise
    pub fn get_output(&self, key: &str) -> Option<&StageOutput> {
        self.outputs.iter().find(|o| o.key == key)
    }

    /// Remove an output by key.
    ///
    /// # Arguments
    /// * `key` - The key of the output to remove
    ///
    /// # Returns
    /// `true` if an output was removed, `false` if no output with that key existed
    pub fn remove_output(&mut self, key: &str) -> bool {
        let len_before = self.outputs.len();
        self.outputs.retain(|o| o.key != key);
        if self.outputs.len() != len_before {
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// Check if an output key already exists in this stage.
    ///
    /// # Arguments
    /// * `key` - The key to check
    ///
    /// # Returns
    /// `true` if the key exists, `false` otherwise
    pub fn has_output(&self, key: &str) -> bool {
        self.outputs.iter().any(|o| o.key == key)
    }

    /// Check if this stage is a knowledge-gathering stage.
    ///
    /// A stage is considered a knowledge stage if:
    /// 1. Its `stage_type` is explicitly set to `Knowledge`, OR
    /// 2. Its ID or name contains "knowledge" (case-insensitive)
    ///
    /// This allows both explicit typing via plan YAML and implicit
    /// detection based on naming conventions.
    pub fn is_knowledge_stage(&self) -> bool {
        self.stage_type == StageType::Knowledge
            || self.id.to_lowercase().contains("knowledge")
            || self.name.to_lowercase().contains("knowledge")
    }

    /// Check if this stage has any goal-backward verification checks defined.
    pub fn has_any_goal_checks(&self) -> bool {
        !self.truths.is_empty()
            || !self.artifacts.is_empty()
            || !self.wiring.is_empty()
            || !self.truth_checks.is_empty()
            || !self.wiring_tests.is_empty()
            || self.dead_code_check.is_some()
    }

    /// Begin a new execution attempt.
    ///
    /// Sets `attempt_started_at` to the given timestamp and initializes
    /// `execution_secs` to 0 if not already set.
    pub fn begin_attempt(&mut self, now: DateTime<Utc>) {
        self.attempt_started_at = Some(now);
        if self.execution_secs.is_none() {
            self.execution_secs = Some(0);
        }
    }

    /// Accumulate time from the current execution attempt.
    ///
    /// Calculates elapsed time since `attempt_started_at`, adds it to
    /// `execution_secs`, and clears `attempt_started_at`.
    /// No-op if `attempt_started_at` is None.
    pub fn accumulate_attempt_time(&mut self, now: DateTime<Utc>) {
        if let Some(start) = self.attempt_started_at.take() {
            let elapsed = now.signed_duration_since(start).num_seconds().max(0);
            let current = self.execution_secs.unwrap_or(0);
            self.execution_secs = Some(current.saturating_add(elapsed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_begin_attempt_initializes_execution_secs() {
        let mut stage = Stage::default();
        assert!(stage.execution_secs.is_none());
        assert!(stage.attempt_started_at.is_none());

        let now = Utc::now();
        stage.begin_attempt(now);

        assert_eq!(stage.execution_secs, Some(0));
        assert_eq!(stage.attempt_started_at, Some(now));
    }

    #[test]
    fn test_begin_attempt_preserves_existing_execution_secs() {
        let mut stage = Stage {
            execution_secs: Some(60),
            ..Stage::default()
        };

        let now = Utc::now();
        stage.begin_attempt(now);

        assert_eq!(stage.execution_secs, Some(60)); // Preserved
        assert_eq!(stage.attempt_started_at, Some(now));
    }

    #[test]
    fn test_accumulate_attempt_time() {
        let mut stage = Stage::default();
        let start = Utc::now() - Duration::seconds(30);
        stage.attempt_started_at = Some(start);
        stage.execution_secs = Some(0);

        stage.accumulate_attempt_time(Utc::now());

        assert!(stage.execution_secs.unwrap() >= 29); // Allow for timing variance
        assert!(stage.attempt_started_at.is_none()); // Cleared
    }

    #[test]
    fn test_accumulate_attempt_time_adds_to_existing() {
        let start = Utc::now() - Duration::seconds(50);
        let mut stage = Stage {
            execution_secs: Some(100),
            attempt_started_at: Some(start),
            ..Stage::default()
        };

        stage.accumulate_attempt_time(Utc::now());

        assert!(stage.execution_secs.unwrap() >= 149); // 100 + ~50
        assert!(stage.attempt_started_at.is_none());
    }

    #[test]
    fn test_accumulate_attempt_time_noop_without_started() {
        let mut stage = Stage {
            execution_secs: Some(100),
            ..Stage::default()
        };
        // attempt_started_at is None

        stage.accumulate_attempt_time(Utc::now());

        assert_eq!(stage.execution_secs, Some(100)); // Unchanged
    }
}
