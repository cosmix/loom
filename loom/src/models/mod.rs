pub mod constants;
pub mod failure;
pub mod keys;
pub mod plan;
pub mod runner;
pub mod serialization;
pub mod session;
pub mod signal;
pub mod stage;
pub mod track;
pub mod worktree;

// Handoff module is currently defined but not actively used.
// It provides the data model for context handoffs between runners,
// which will be implemented in a future feature for the `loom handoff` command.
// See: https://github.com/cosmix/loom/issues/XXX (future work)
#[allow(dead_code)]
pub mod handoff;

pub use serialization::MarkdownSerializable;
