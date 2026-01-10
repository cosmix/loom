mod base_conflict;
mod cache;
mod crud;
mod format;
mod generate;
mod merge;
mod parse;
mod types;

#[cfg(test)]
mod tests;

// Re-export public types
pub use cache::SignalMetrics;
pub use types::{
    BaseConflictSignalContent, DependencyStatus, EmbeddedContext, MergeSignalContent,
    SignalContent, SignalUpdates, TaskStatus,
};

// Re-export public functions
pub use base_conflict::{generate_base_conflict_signal, read_base_conflict_signal};
pub use cache::compute_hash;
pub use crud::{list_signals, read_signal, remove_signal, update_signal};
pub use format::{format_dependency_table, format_signal_with_metrics, FormattedSignal};
pub use generate::{build_embedded_context_with_stage, generate_signal, generate_signal_with_metrics};
pub use merge::{generate_merge_signal, read_merge_signal};
