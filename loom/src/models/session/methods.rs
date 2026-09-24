use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

use super::types::{Session, SessionBackendKind, SessionExitReason, SessionStatus, SessionType};

impl Session {
    pub fn new() -> Self {
        let now = Utc::now();
        let id = Self::generate_id();

        Self {
            id,
            stage_id: None,
            worktree_path: None,
            pid: None,
            status: SessionStatus::Spawning,
            exit_reason: None,
            context_tokens: 0,
            transcript_path: None,
            created_at: now,
            last_active: now,
            session_type: SessionType::default(),
            merge_source_branch: None,
            merge_target_branch: None,
            tracking_key: String::new(),
            backend: SessionBackendKind::default(),
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

    /// Create a new knowledge-gathering session.
    ///
    /// Knowledge sessions run in the main repository (no worktree) and
    /// populate `doc/loom/knowledge/`. The `stage_id` is required so the
    /// `tracking_key` can be derived up-front (see
    /// [`Session::derive_tracking_key`]).
    pub fn new_knowledge(stage_id: &str) -> Self {
        let mut session = Self::new();
        session.session_type = SessionType::Knowledge;
        session.stage_id = Some(stage_id.to_string());
        session.tracking_key = Self::derive_tracking_key(stage_id, SessionType::Knowledge);
        session
    }

    /// Create a new adjudication session for a disputed criterion.
    ///
    /// Adjudication sessions run in the main repository (no worktree), judge
    /// one dispute, and report their verdict through `loom stage adjudicate`.
    /// The `stage_id` is required so the `tracking_key` can be derived up-front
    /// (see [`Session::derive_tracking_key`]).
    pub fn new_adjudication(stage_id: &str) -> Self {
        let mut session = Self::new();
        session.session_type = SessionType::Adjudication;
        session.stage_id = Some(stage_id.to_string());
        session.tracking_key = Self::derive_tracking_key(stage_id, SessionType::Adjudication);
        session
    }

    /// Create a new contract session: the agent that writes a v2 stage's
    /// failing contract tests in the stage worktree before its `Stage`
    /// session starts. The `stage_id` is required so the `tracking_key` can
    /// be derived up-front (see [`Session::derive_tracking_key`]).
    pub fn new_contract(stage_id: &str) -> Self {
        let mut session = Self::new();
        session.session_type = SessionType::Contract;
        session.stage_id = Some(stage_id.to_string());
        session.tracking_key = Self::derive_tracking_key(stage_id, SessionType::Contract);
        session
    }

    /// Derive the canonical tracking key for a session.
    ///
    /// The tracking key is used to find OS-level resources owned by this
    /// session (terminal window titles, etc.) without having to thread the
    /// session ID through every spawn/kill code path.
    ///
    /// Format: `loom-[<kind>-]<stage_id>` where `<kind>` is omitted for
    /// regular stage sessions.
    pub fn derive_tracking_key(stage_id: &str, kind: SessionType) -> String {
        match kind {
            SessionType::Stage => format!("loom-{stage_id}"),
            SessionType::Merge => format!("loom-merge-{stage_id}"),
            SessionType::BaseConflict => format!("loom-base-conflict-{stage_id}"),
            SessionType::Knowledge => format!("loom-knowledge-{stage_id}"),
            SessionType::Adjudication => format!("loom-adjudication-{stage_id}"),
            SessionType::Contract => format!("loom-contract-{stage_id}"),
        }
    }

    /// Check if this is a merge resolution session
    pub fn is_merge_session(&self) -> bool {
        self.session_type == SessionType::Merge
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
        // Derive the tracking key from the (stage_id, session_type) pair so
        // OS-level resource lookups (terminal titles) have a stable handle
        // even before the session has a PID.
        self.tracking_key = Self::derive_tracking_key(&stage_id, self.session_type);
        self.stage_id = Some(stage_id);
        self.last_active = Utc::now();
    }

    pub fn set_worktree_path(&mut self, path: PathBuf) {
        self.worktree_path = Some(path);
    }

    pub fn set_pid(&mut self, pid: u32) {
        self.pid = Some(pid);
    }

    /// Apply an observed heartbeat to this session.
    ///
    /// A heartbeat is the only ongoing evidence the daemon has that a live
    /// session is doing anything: the hooks rewrite
    /// `.loom/work/heartbeat/<stage-id>.json` after every tool call. Recording it
    /// here is what keeps `last_active` honest — without it the field is set
    /// once by [`Session::assign_to_stage`] at spawn and never again, so every
    /// duration derived from it reports a session's entire lifetime as idle.
    ///
    /// `None` for either reading PRESERVES what a previous heartbeat
    /// established. A hook that cannot measure the transcript on this tick is
    /// reporting ignorance, not a context of zero, and treating the two alike
    /// would silently retract a handoff that was already due.
    pub fn record_heartbeat(
        &mut self,
        progress_at: DateTime<Utc>,
        context_tokens: Option<u32>,
        transcript_path: Option<String>,
    ) {
        self.last_active = self.last_active.max(progress_at);

        if let Some(tokens) = context_tokens {
            self.context_tokens = tokens;
        }
        if let Some(path) = transcript_path {
            self.transcript_path = Some(path);
        }
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
        self.try_mark_terminal(SessionStatus::Completed, SessionExitReason::Completed)
    }

    /// Mark the session as crashed with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_crashed(&mut self) -> Result<()> {
        self.try_mark_terminal(SessionStatus::Crashed, SessionExitReason::Crashed)
    }

    /// Mark the session as context exhausted with validation.
    ///
    /// # Returns
    /// `Ok(())` if the transition succeeded, `Err` if invalid
    pub fn try_mark_context_exhausted(&mut self) -> Result<()> {
        self.try_mark_terminal(
            SessionStatus::ContextExhausted,
            SessionExitReason::ContextCeiling,
        )
    }

    /// Apply an explicit terminal status and cause while preserving the first
    /// durable reason recorded for the session.
    pub fn try_mark_terminal(
        &mut self,
        status: SessionStatus,
        reason: SessionExitReason,
    ) -> Result<()> {
        if !status.is_terminal() {
            anyhow::bail!("Terminal session transition requires a terminal status");
        }
        self.try_transition(status)?;
        if self.exit_reason.is_none() {
            self.exit_reason = Some(reason);
        }
        Ok(())
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
