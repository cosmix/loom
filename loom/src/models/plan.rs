use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Plan is a container for stages extracted from a plan document.
/// Plans live in doc/plans/PLAN-xxx.md and contain structured YAML metadata
/// defining stages, their dependencies, and acceptance criteria.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_path: String,
    pub stages: Vec<StageRef>,
    pub status: PlanStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Plan {
    /// Creates a new Plan in Draft status
    pub fn new(id: String, name: String, source_path: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            description: None,
            source_path,
            stages: Vec::new(),
            status: PlanStatus::Draft,
            created_at: now,
            updated_at: now,
        }
    }

    /// Adds a stage reference to the plan
    pub fn add_stage(&mut self, stage_ref: StageRef) {
        self.stages.push(stage_ref);
        self.update_timestamp();
    }

    /// Removes a stage by ID, returns true if the stage was found and removed
    pub fn remove_stage(&mut self, stage_id: &str) -> bool {
        let initial_len = self.stages.len();
        self.stages.retain(|s| s.id != stage_id);
        let removed = self.stages.len() < initial_len;
        if removed {
            self.update_timestamp();
        }
        removed
    }

    /// Gets a stage reference by ID
    pub fn get_stage(&self, stage_id: &str) -> Option<&StageRef> {
        self.stages.iter().find(|s| s.id == stage_id)
    }

    /// Returns the number of stages in the plan
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Marks the plan as ready for execution
    pub fn mark_ready(&mut self) {
        self.status = PlanStatus::Ready;
        self.update_timestamp();
    }

    /// Marks the plan as currently executing
    pub fn mark_executing(&mut self) {
        self.status = PlanStatus::Executing;
        self.update_timestamp();
    }

    /// Marks the plan as paused
    pub fn mark_paused(&mut self) {
        self.status = PlanStatus::Paused;
        self.update_timestamp();
    }

    /// Marks the plan as completed
    pub fn mark_completed(&mut self) {
        self.status = PlanStatus::Completed;
        self.update_timestamp();
    }

    /// Marks the plan as failed
    pub fn mark_failed(&mut self) {
        self.status = PlanStatus::Failed;
        self.update_timestamp();
    }

    /// Updates the updated_at timestamp to the current time
    pub fn update_timestamp(&mut self) {
        self.updated_at = Utc::now();
    }
}

/// Lightweight reference to a stage within a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRef {
    pub id: String,
    pub name: String,
    pub parallel_group: Option<String>,
}

impl StageRef {
    /// Creates a new stage reference
    pub fn new(id: String, name: String, parallel_group: Option<String>) -> Self {
        Self {
            id,
            name,
            parallel_group,
        }
    }
}

/// Status of a plan in its lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    /// Plan is being created/edited
    Draft,
    /// Plan is ready for execution
    Ready,
    /// Plan is currently being executed
    Executing,
    /// Plan execution is paused
    Paused,
    /// All stages completed
    Completed,
    /// Plan failed (one or more stages failed)
    Failed,
}
