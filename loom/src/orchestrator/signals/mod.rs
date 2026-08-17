mod cache;
mod crud;
mod format;
mod generate;
mod helpers;
mod knowledge;
mod merge;
mod merge_conflict;
mod parse;
mod recovery;
mod recovery_format;
mod recovery_parsing;
mod recovery_types;
mod retrieval;
mod section_formatters;
mod types;

#[cfg(test)]
mod tests;

// Re-export public types
pub use cache::SignalMetrics;
pub use recovery::generate_recovery_signal;
pub use recovery_parsing::read_recovery_signal;
pub use recovery_types::{LastHeartbeatInfo, RecoveryReason, RecoverySignalContent};
pub use types::{
    DependencyStatus, EmbeddedContext, MergeConflictSignalContent, MergeSignalContent,
    SignalContent, SignalUpdates,
};

// Re-export public functions
pub use cache::compute_hash;
pub use crud::{list_signals, read_signal, remove_signal, update_signal};
pub use format::{
    format_dependency_table, format_signal_with_metrics, format_skill_recommendations,
    FormattedSignal,
};
// Crate-internal re-exports. `mod cache` and `mod format` are private, and
// visibility is capped by path reachability, so a `pub(crate)` item inside
// them is unreachable from outside `signals/` unless it is re-exported here.
// Both of these have consumers in other modules: the session launcher writes
// the stable prefix to its own file for the prompt-cache split
// (`orchestrator::terminal::native::launch`), and the UserPromptSubmit hook
// renders its brief with the SAME untrusted-excerpt fencing rules the signal
// path uses (`commands::hook::user_prompt`) rather than a second copy.
pub(crate) use cache::stable_prefix_for;
pub(crate) use format::format_knowledge_brief;
pub use generate::{
    build_embedded_context_with_stage, generate_signal, generate_signal_with_metrics,
    generate_signal_with_skills, DEFAULT_MAX_SKILL_RECOMMENDATIONS,
};
pub use knowledge::generate_knowledge_signal;
pub use merge::{find_live_merge_session_for_stage, generate_merge_signal, read_merge_signal};
pub use merge_conflict::{generate_merge_conflict_signal, read_merge_conflict_signal};
