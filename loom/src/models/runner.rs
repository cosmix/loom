use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::constants::DEFAULT_CONTEXT_LIMIT;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runner {
    pub id: String,
    pub name: String,
    pub runner_type: String,
    pub status: RunnerStatus,
    pub assigned_track: Option<String>,
    pub context_tokens: u32,
    pub context_limit: u32,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunnerStatus {
    Idle,
    Active,
    Blocked,
    Archived,
}

impl Runner {
    pub fn new(name: String, runner_type: String) -> Self {
        let now = Utc::now();
        let id = Self::generate_id(&name);

        Self {
            id,
            name,
            runner_type,
            status: RunnerStatus::Idle,
            assigned_track: None,
            context_tokens: 0,
            context_limit: DEFAULT_CONTEXT_LIMIT,
            created_at: now,
            last_active: now,
        }
    }

    fn generate_id(name: &str) -> String {
        let timestamp = Utc::now().timestamp();
        format!(
            "runner-{}-{}",
            name.to_lowercase().replace(' ', "-"),
            timestamp
        )
    }

    pub fn assign_to_track(&mut self, track_id: String) {
        self.assigned_track = Some(track_id);
        self.status = RunnerStatus::Active;
        self.last_active = Utc::now();
    }

    pub fn release_from_track(&mut self) {
        self.assigned_track = None;
        self.status = RunnerStatus::Idle;
        self.last_active = Utc::now();
    }

    pub fn update_context(&mut self, tokens: u32) {
        self.context_tokens = tokens;
        self.last_active = Utc::now();
    }

    pub fn context_usage_percent(&self) -> f32 {
        if self.context_limit == 0 {
            return 0.0;
        }
        (self.context_tokens as f32 / self.context_limit as f32) * 100.0
    }

    pub fn archive(&mut self) {
        self.status = RunnerStatus::Archived;
        self.last_active = Utc::now();
    }
}
