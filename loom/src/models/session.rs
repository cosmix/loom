use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::constants::{CONTEXT_WARNING_THRESHOLD, DEFAULT_CONTEXT_LIMIT};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub stage_id: Option<String>,
    pub tmux_session: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub pid: Option<u32>,
    pub status: SessionStatus,
    pub context_tokens: u32,
    pub context_limit: u32,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Spawning,
    Running,
    Paused,
    Completed,
    Crashed,
    ContextExhausted,
}

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
        }
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

    pub fn mark_running(&mut self) {
        self.status = SessionStatus::Running;
    }

    pub fn mark_paused(&mut self) {
        self.status = SessionStatus::Paused;
    }

    pub fn mark_completed(&mut self) {
        self.status = SessionStatus::Completed;
    }

    pub fn mark_crashed(&mut self) {
        self.status = SessionStatus::Crashed;
    }

    pub fn mark_context_exhausted(&mut self) {
        self.status = SessionStatus::ContextExhausted;
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
