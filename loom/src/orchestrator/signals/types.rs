use crate::handoff::git_handoff::GitHistory;

/// Embedded context to include directly in signals so agents don't need to read from main repo
#[derive(Debug, Clone, Default)]
pub struct EmbeddedContext {
    /// Content of the handoff file (if resuming from a previous session)
    pub handoff_content: Option<String>,
    /// Content of structure.md (codebase structure map)
    pub structure_content: Option<String>,
    /// Plan overview extracted from the plan file
    pub plan_overview: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DependencyStatus {
    pub stage_id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct SignalContent {
    pub session_id: String,
    pub stage_id: String,
    pub plan_id: Option<String>,
    pub stage_name: String,
    pub description: String,
    pub tasks: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub context_files: Vec<String>,
    pub files_to_modify: Vec<String>,
    pub git_history: Option<GitHistory>,
}

/// Content for a merge conflict resolution signal
#[derive(Debug, Clone)]
pub struct MergeSignalContent {
    pub session_id: String,
    pub stage_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub conflicting_files: Vec<String>,
}

#[derive(Debug, Default)]
pub struct SignalUpdates {
    pub add_tasks: Option<Vec<String>>,
    pub update_dependencies: Option<Vec<DependencyStatus>>,
    pub add_context_files: Option<Vec<String>>,
}
