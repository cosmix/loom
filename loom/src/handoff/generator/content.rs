//! Handoff content data structures and builder methods.

use crate::handoff::git_handoff::GitHistory;
use crate::handoff::schema::{CommitRef, CompletedTask, HandoffV2, KeyDecision};

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
    pub git_history: Option<GitHistory>,
    /// Memory content from session journal (for handoff)
    pub memory_content: Option<String>,
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
            git_history: None,
            memory_content: None,
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

    /// Set memory content from session journal
    pub fn with_memory_content(mut self, content: Option<String>) -> Self {
        self.memory_content = content;
        self
    }

    /// Convert to HandoffV2 structured format
    pub fn to_v2(&self) -> HandoffV2 {
        // Convert completed_work strings to CompletedTask structs
        let completed_tasks: Vec<CompletedTask> = self
            .completed_work
            .iter()
            .map(|work| CompletedTask::new(work.clone()))
            .collect();

        // Convert decisions tuples to KeyDecision structs
        let key_decisions: Vec<KeyDecision> = self
            .decisions
            .iter()
            .map(|(decision, rationale)| KeyDecision::new(decision.clone(), rationale.clone()))
            .collect();

        // Convert git history commits if available
        let commits: Vec<CommitRef> = self
            .git_history
            .as_ref()
            .map(|h| {
                h.commits
                    .iter()
                    .map(|c| CommitRef::new(c.hash.clone(), c.message.clone()))
                    .collect()
            })
            .unwrap_or_default();

        // Get uncommitted files from git history
        let uncommitted_files: Vec<String> = self
            .git_history
            .as_ref()
            .map(|h| h.uncommitted_changes.clone())
            .unwrap_or_default();

        HandoffV2::new(&self.session_id, &self.stage_id)
            .with_context_percent(self.context_percent)
            .with_completed_tasks(completed_tasks)
            .with_key_decisions(key_decisions)
            .with_next_actions(self.next_steps.clone())
            .with_branch(
                self.current_branch
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .with_commits(commits)
            .with_uncommitted_files(uncommitted_files)
            .with_files_modified(self.files_modified.clone())
    }
}
