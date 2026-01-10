mod base_conflict;
mod crud;
mod format;
mod generate;
mod merge;
mod parse;
mod types;

#[cfg(test)]
mod tests;

// Re-export public types
pub use types::{
    BaseConflictSignalContent, DependencyStatus, EmbeddedContext, MergeSignalContent,
    SignalContent, SignalUpdates,
};

// Re-export public functions
pub use base_conflict::{generate_base_conflict_signal, read_base_conflict_signal};
pub use crud::{list_signals, read_signal, remove_signal, update_signal};
pub use format::format_dependency_table;
pub use generate::generate_signal;
pub use merge::{generate_merge_signal, read_merge_signal};
