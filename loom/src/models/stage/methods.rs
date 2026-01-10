use anyhow::Result;
use chrono::Utc;

use super::types::{Stage, StageStatus};

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
            auto_merge: None,
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            base_branch: None,
            base_merged_from: Vec::new(),
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

    /// Mark the stage as skipped with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_skip(&mut self, reason: Option<String>) -> Result<()> {
        self.try_transition(StageStatus::Skipped)?;
        self.close_reason = reason;
        Ok(())
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
