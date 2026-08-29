use crate::handoff::git_handoff::GitHistory;
use crate::handoff::schema::HandoffV2;
use crate::models::stage::{Implementers, StageOutput};
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
    /// The retrieved brief for this stage, when retrieval succeeded and selected
    /// anything. `None` means the signal carries no brief — never a silent empty
    /// section.
    pub context_pack: Option<crate::context::schema::ContextPack>,
    /// Whether the project's `doc/loom/knowledge/` tree holds no real content,
    /// resolved once where the context is built.
    ///
    /// Deliberately distinct from `context_pack`: a `None` pack only says THIS
    /// stage's retrieval selected nothing, which also happens when retrieval
    /// merely degrades, so it can never gate the "KNOWLEDGE BASE IS EMPTY" box
    /// without telling an agent with a fully populated tree to go document a
    /// codebase that is already documented. Defaults to `false` — never claim
    /// emptiness that was not established.
    pub knowledge_tree_empty: bool,
    /// Recent memory entries for recitation (Manus pattern - keeps context in attention)
    pub memory_content: Option<String>,
    /// Skill recommendations based on stage description matching
    pub skill_recommendations: Vec<SkillMatch>,
    /// Effective context ceiling for this stage, in resident tokens.
    pub context_ceiling_tokens: Option<u32>,
    /// Current resident context size, in tokens.
    pub context_tokens: Option<u32>,
    /// Merged sandbox configuration summary for display in signal
    pub sandbox_summary: Option<SandboxSummary>,
    /// Cross-stage change summary for integration-verify stages
    pub cross_stage_summary: Option<String>,
    /// Memory-based wiring checklist aggregated from all stages
    pub wiring_checklist: Option<String>,
    /// Whether the stage is licensed for ultracode Workflow orchestration
    pub ultracode: bool,
    /// Which agent lanes this stage may spawn subagents from, in preference order
    pub implementers: Implementers,
    /// Whether the codex CLI and its plugin's companion runtime are installed
    /// on this machine, resolved once at context-build time. Gates the Codex
    /// Implementers section between the full doctrine and the route-to-sonnet
    /// fallback; the formatters never probe the machine themselves.
    pub codex_available: bool,
    /// Per-subagent response budget in seconds, when the stage sets one
    /// explicitly. `None` leaves the built-in default in force and emits
    /// nothing — the signal only spends tokens on a budget the plan chose.
    pub subagent_timeout_secs: Option<u64>,
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
