use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub id: String,
    pub from_runner: String,
    pub to_runner: Option<String>,
    pub track_id: String,
    pub context_summary: String,
    pub pending_tasks: Vec<String>,
    pub blocker_info: Option<String>,
    pub status: HandoffStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HandoffStatus {
    Pending,
    Accepted,
    Rejected,
    Completed,
}

impl Handoff {
    pub fn new(
        from_runner: String,
        track_id: String,
        context_summary: String,
        pending_tasks: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        let id = Self::generate_id(&track_id);

        Self {
            id,
            from_runner,
            to_runner: None,
            track_id,
            context_summary,
            pending_tasks,
            blocker_info: None,
            status: HandoffStatus::Pending,
            created_at: now,
            completed_at: None,
        }
    }

    fn generate_id(track_id: &str) -> String {
        let timestamp = Utc::now().timestamp();
        format!("handoff-{track_id}-{timestamp}")
    }

    pub fn assign_to_runner(&mut self, runner_id: String) {
        self.to_runner = Some(runner_id);
        self.status = HandoffStatus::Accepted;
    }

    pub fn reject(&mut self) {
        self.status = HandoffStatus::Rejected;
    }

    pub fn complete(&mut self) {
        self.status = HandoffStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn add_blocker(&mut self, blocker: String) {
        self.blocker_info = Some(blocker);
    }
}
