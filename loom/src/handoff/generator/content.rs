//! Handoff content data structures and builder methods.

use crate::handoff::git_handoff::GitHistory;

/// Content for generating a handoff file
#[derive(Debug, Clone)]
pub struct HandoffContent {
    pub session_id: String,
    pub stage_id: String,
    pub plan_id: Option<String>,
    pub context_percent: f32,
    pub goals: String,
    pub completed_work: Vec<String>,
    pub decisions: Vec<(String, String)>, // (decision, rationale)
    pub current_branch: Option<String>,
    pub test_status: Option<String>,
    pub files_modified: Vec<String>,
    pub next_steps: Vec<String>,
    pub learnings: Vec<String>,
    pub git_history: Option<GitHistory>,
}

impl HandoffContent {
    /// Create a new HandoffContent with minimal fields
    pub fn new(session_id: String, stage_id: String) -> Self {
        Self {
            session_id,
            stage_id,
            plan_id: None,
            context_percent: 0.0,
            goals: String::new(),
            completed_work: Vec::new(),
            decisions: Vec::new(),
            current_branch: None,
            test_status: None,
            files_modified: Vec::new(),
            next_steps: Vec::new(),
            learnings: Vec::new(),
            git_history: None,
        }
    }

    /// Set the context usage percentage
    pub fn with_context_percent(mut self, percent: f32) -> Self {
        self.context_percent = percent;
        self
    }

    /// Set the overall goals
    pub fn with_goals(mut self, goals: String) -> Self {
        self.goals = goals;
        self
    }

    /// Add completed work items
    pub fn with_completed_work(mut self, work: Vec<String>) -> Self {
        self.completed_work = work;
        self
    }

    /// Add decisions made
    pub fn with_decisions(mut self, decisions: Vec<(String, String)>) -> Self {
        self.decisions = decisions;
        self
    }

    /// Set current branch
    pub fn with_current_branch(mut self, branch: Option<String>) -> Self {
        self.current_branch = branch;
        self
    }

    /// Set test status
    pub fn with_test_status(mut self, status: Option<String>) -> Self {
        self.test_status = status;
        self
    }

    /// Add modified files
    pub fn with_files_modified(mut self, files: Vec<String>) -> Self {
        self.files_modified = files;
        self
    }

    /// Add next steps
    pub fn with_next_steps(mut self, steps: Vec<String>) -> Self {
        self.next_steps = steps;
        self
    }

    /// Add learnings
    pub fn with_learnings(mut self, learnings: Vec<String>) -> Self {
        self.learnings = learnings;
        self
    }

    /// Set plan ID
    pub fn with_plan_id(mut self, plan_id: Option<String>) -> Self {
        self.plan_id = plan_id;
        self
    }

    /// Set git history
    pub fn with_git_history(mut self, history: Option<GitHistory>) -> Self {
        self.git_history = history;
        self
    }
}
