use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: TrackStatus,
    pub assigned_runner: Option<String>,
    pub parent_track: Option<String>,
    pub child_tracks: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrackStatus {
    Active,
    Blocked,
    Completed,
    Archived,
}

impl Track {
    pub fn new(name: String, description: Option<String>) -> Self {
        let now = Utc::now();
        let id = Self::generate_id(&name);

        Self {
            id,
            name,
            description,
            status: TrackStatus::Active,
            assigned_runner: None,
            parent_track: None,
            child_tracks: Vec::new(),
            created_at: now,
            updated_at: now,
            closed_at: None,
            close_reason: None,
        }
    }

    fn generate_id(name: &str) -> String {
        let timestamp = Utc::now().timestamp();
        format!(
            "track-{}-{}",
            name.to_lowercase().replace(' ', "-"),
            timestamp
        )
    }

    pub fn assign_runner(&mut self, runner_id: String) {
        self.assigned_runner = Some(runner_id);
        self.updated_at = Utc::now();
    }

    pub fn add_child_track(&mut self, child_id: String) {
        self.child_tracks.push(child_id);
        self.updated_at = Utc::now();
    }

    pub fn close(&mut self, reason: Option<String>) {
        self.status = TrackStatus::Completed;
        self.closed_at = Some(Utc::now());
        self.close_reason = reason;
        self.updated_at = Utc::now();
    }

    pub fn archive(&mut self) {
        self.status = TrackStatus::Archived;
        self.updated_at = Utc::now();
    }
}
