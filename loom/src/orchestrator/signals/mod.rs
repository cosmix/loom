mod crud;
mod format;
mod generate;
mod merge;
mod parse;
mod types;

#[cfg(test)]
mod tests;

// Re-export public types
pub use types::{DependencyStatus, EmbeddedContext, MergeSignalContent, SignalContent, SignalUpdates};

// Re-export public functions
pub use crud::{list_signals, read_signal, remove_signal, update_signal};
pub use format::format_dependency_table;
pub use generate::generate_signal;
pub use merge::{generate_merge_signal, read_merge_signal};
