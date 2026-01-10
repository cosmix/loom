use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub id: String,
    pub target_runner: String,
    pub signal_type: String,
    pub message: String,
    pub priority: u8,
    pub status: SignalStatus,
    pub created_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalStatus {
    Pending,
    Acknowledged,
    Completed,
}

impl Signal {
    pub fn new(target_runner: String, signal_type: String, message: String, priority: u8) -> Self {
        let now = Utc::now();
        let id = Self::generate_id(&target_runner);

        Self {
            id,
            target_runner,
            signal_type,
            message,
            priority: priority.clamp(1, 5),
            status: SignalStatus::Pending,
            created_at: now,
            acknowledged_at: None,
        }
    }

    fn generate_id(runner: &str) -> String {
        let timestamp = Utc::now().timestamp();
        format!("signal-{runner}-{timestamp}")
    }

    pub fn acknowledge(&mut self) {
        self.status = SignalStatus::Acknowledged;
        self.acknowledged_at = Some(Utc::now());
    }

    pub fn complete(&mut self) {
        self.status = SignalStatus::Completed;
    }
}
