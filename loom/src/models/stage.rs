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
    pub files: Vec<String>,
    pub plan_id: Option<String>,
    pub worktree: Option<String>,
    pub session: Option<String>,
    pub parent_stage: Option<String>,
    pub child_stages: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StageStatus {
    Pending,
    Ready,
    Executing,
    #[serde(rename = "waiting-for-input")]
    WaitingForInput,
    Blocked,
    Completed,
    NeedsHandoff,
    Verified,
}

impl Stage {
    pub fn new(name: String, description: Option<String>) -> Self {
        let now = Utc::now();
        let id = Self::generate_id(&name);

        Self {
            id,
            name,
            description,
            status: StageStatus::Pending,
            dependencies: Vec::new(),
            parallel_group: None,
            acceptance: Vec::new(),
            files: Vec::new(),
            plan_id: None,
            worktree: None,
            session: None,
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

    pub fn complete(&mut self, reason: Option<String>) {
        self.status = StageStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.close_reason = reason;
        self.updated_at = Utc::now();
    }

    pub fn mark_verified(&mut self) {
        self.status = StageStatus::Verified;
        self.updated_at = Utc::now();
    }

    pub fn mark_needs_handoff(&mut self) {
        self.status = StageStatus::NeedsHandoff;
        self.updated_at = Utc::now();
    }

    pub fn mark_ready(&mut self) {
        self.status = StageStatus::Ready;
        self.updated_at = Utc::now();
    }

    pub fn mark_executing(&mut self) {
        self.status = StageStatus::Executing;
        self.updated_at = Utc::now();
    }

    pub fn mark_waiting_for_input(&mut self) {
        self.status = StageStatus::WaitingForInput;
        self.updated_at = Utc::now();
    }
}
