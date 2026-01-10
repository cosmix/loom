use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;

use super::types::{Session, SessionStatus, SessionType};
use crate::models::constants::{CONTEXT_WARNING_THRESHOLD, DEFAULT_CONTEXT_LIMIT};

impl Session {
    pub fn new() -> Self {
        let now = Utc::now();
        let id = Self::generate_id();

        Self {
            id,
            stage_id: None,
            tmux_session: None,
            worktree_path: None,
            pid: None,
            status: SessionStatus::Spawning,
            context_tokens: 0,
            context_limit: DEFAULT_CONTEXT_LIMIT,
            created_at: now,
            last_active: now,
            session_type: SessionType::default(),
            merge_source_branch: None,
            merge_target_branch: None,
        }
    }

    /// Create a new merge conflict resolution session
    pub fn new_merge(source_branch: String, target_branch: String) -> Self {
        let mut session = Self::new();
        session.session_type = SessionType::Merge;
        session.merge_source_branch = Some(source_branch);
        session.merge_target_branch = Some(target_branch);
        session
    }

    /// Create a new base branch conflict resolution session
    ///
    /// Used when merging multiple dependency branches into a base branch
    /// (loom/_base/{stage_id}) fails due to conflicts.
    pub fn new_base_conflict(target_branch: String) -> Self {
        let mut session = Self::new();
        session.session_type = SessionType::BaseConflict;
        session.merge_target_branch = Some(target_branch);
        session
    }

    /// Check if this is a merge resolution session
    pub fn is_merge_session(&self) -> bool {
        self.session_type == SessionType::Merge
    }

    /// Check if this is a base conflict resolution session
    pub fn is_base_conflict_session(&self) -> bool {
        self.session_type == SessionType::BaseConflict
    }

    fn generate_id() -> String {
        let timestamp = Utc::now().timestamp();
        let uuid_short = uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("")
            .to_string();
        format!("session-{uuid_short}-{timestamp}")
    }

    pub fn assign_to_stage(&mut self, stage_id: String) {
        self.stage_id = Some(stage_id);
        self.last_active = Utc::now();
    }

    pub fn release_from_stage(&mut self) {
        self.stage_id = None;
        self.last_active = Utc::now();
    }

    pub fn set_tmux_session(&mut self, session_name: String) {
        self.tmux_session = Some(session_name);
    }

    pub fn set_worktree_path(&mut self, path: PathBuf) {
        self.worktree_path = Some(path);
    }

    pub fn set_pid(&mut self, pid: u32) {
        self.pid = Some(pid);
    }

    pub fn clear_pid(&mut self) {
        self.pid = None;
    }

    pub fn update_context(&mut self, tokens: u32) {
        self.context_tokens = tokens;
        self.last_active = Utc::now();
    }

    pub fn context_health(&self) -> f32 {
        if self.context_limit == 0 {
            return 0.0;
        }
        (self.context_tokens as f32 / self.context_limit as f32) * 100.0
    }

    pub fn is_context_exhausted(&self) -> bool {
        if self.context_limit == 0 {
            return false;
        }
        let usage_fraction = self.context_tokens as f32 / self.context_limit as f32;
        usage_fraction >= CONTEXT_WARNING_THRESHOLD
    }

    /// Attempt to transition the session to a new status with validation.
    ///
    /// This is the primary method for changing session status. It validates
    /// that the transition is allowed before applying it.
    ///
    /// # Arguments
    /// * `new_status` - The target status to transition to
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if the transition is invalid
    pub fn try_transition(&mut self, new_status: SessionStatus) -> Result<()> {
        let validated_status = self.status.try_transition(new_status)?;
        self.status = validated_status;
        self.last_active = Utc::now();
        Ok(())
    }

    /// Mark the session as running with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_running(&mut self) -> Result<()> {
        self.try_transition(SessionStatus::Running)
    }

    /// Mark the session as paused with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_paused(&mut self) -> Result<()> {
        self.try_transition(SessionStatus::Paused)
    }

    /// Mark the session as completed with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_completed(&mut self) -> Result<()> {
        self.try_transition(SessionStatus::Completed)
    }

    /// Mark the session as crashed with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_crashed(&mut self) -> Result<()> {
        self.try_transition(SessionStatus::Crashed)
    }

    /// Mark the session as context exhausted with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_context_exhausted(&mut self) -> Result<()> {
        self.try_transition(SessionStatus::ContextExhausted)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
