use crate::checkpoints::{TaskDefinition, TaskState};
use crate::handoff::git_handoff::GitHistory;
use crate::handoff::schema::HandoffV2;
use crate::models::stage::StageOutput;
use crate::skills::SkillMatch;

/// Summary of sandbox restrictions for signal display
#[derive(Debug, Clone, Default)]
pub struct SandboxSummary {
    /// Whether sandboxing is enabled
    pub enabled: bool,
    /// Paths agents cannot read
    pub deny_read: Vec<String>,
    /// Paths agents cannot write
    pub deny_write: Vec<String>,
    /// Paths agents are allowed to write (exceptions)
    pub allow_write: Vec<String>,
    /// Allowed network domains
    pub allowed_domains: Vec<String>,
    /// Commands excluded from sandbox
    pub excluded_commands: Vec<String>,
}

/// Embedded context to include directly in signals so agents don't need to read from main repo
#[derive(Debug, Clone, Default)]
pub struct EmbeddedContext {
    /// Content of the handoff file (if resuming from a previous session)
    pub handoff_content: Option<String>,
    /// Parsed V2 handoff data (if available)
    pub parsed_handoff: Option<HandoffV2>,
    /// Plan overview extracted from the plan file
    pub plan_overview: Option<String>,
    /// Whether the knowledge directory has meaningful content
    pub knowledge_has_content: bool,
    /// Task state for the stage (if tasks are defined)
    pub task_state: Option<TaskState>,
    /// Recent memory entries for recitation (Manus pattern - keeps context in attention)
    pub memory_content: Option<String>,
    /// Skill recommendations based on stage description matching
    pub skill_recommendations: Vec<SkillMatch>,
    /// Stage-specific context budget percentage
    pub context_budget: Option<f32>,
    /// Current context usage percentage
    pub context_usage: Option<f32>,
    /// Merged sandbox configuration summary for display in signal
    pub sandbox_summary: Option<SandboxSummary>,
}

/// Information about a task's lock status
#[derive(Debug, Clone)]
pub struct TaskStatus {
    pub task: TaskDefinition,
    pub is_unlocked: bool,
    pub is_completed: bool,
}

#[derive(Debug, Clone)]
pub struct DependencyStatus {
    pub stage_id: String,
    pub name: String,
    pub status: String,
    /// Outputs from the completed dependency stage
    pub outputs: Vec<StageOutput>,
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

/// Content for a base branch conflict resolution signal
///
/// This signal is generated when merging multiple dependency branches into a base branch
/// (loom/_base/{stage_id}) fails due to conflicts. The session runs in the main repository
/// to resolve conflicts before the stage can proceed.
#[derive(Debug, Clone)]
pub struct BaseConflictSignalContent {
    pub session_id: String,
    pub stage_id: String,
    /// The dependency branches being merged
    pub source_branches: Vec<String>,
    /// The target base branch (loom/_base/{stage_id})
    pub target_branch: String,
    /// Files with merge conflicts
    pub conflicting_files: Vec<String>,
}

/// Content for a merge conflict resolution signal (stage MergeConflict status)
///
/// This signal is generated when a stage transitions to MergeConflict status
/// because progressive merge detected conflicts. Unlike MergeSignalContent which
/// is for auto-merge conflicts, this is specifically for stages in MergeConflict
/// status that need dedicated resolution sessions.
#[derive(Debug, Clone)]
pub struct MergeConflictSignalContent {
    pub session_id: String,
    pub stage_id: String,
    /// The target branch to merge into (usually "main" or base_branch from config)
    pub merge_point: String,
    /// Files with merge conflicts
    pub conflicting_files: Vec<String>,
}

#[derive(Debug, Default)]
pub struct SignalUpdates {
    pub add_tasks: Option<Vec<String>>,
    pub update_dependencies: Option<Vec<DependencyStatus>>,
    pub add_context_files: Option<Vec<String>>,
}
