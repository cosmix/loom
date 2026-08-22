//! `loom hook` — deterministic entry points for loom's shell hooks.
//!
//! The shell side of a hook decides *when* to ask; everything it asks is
//! answered here, from the filesystem alone. No subcommand under this module
//! may make a model call or a network call.

pub mod git_repo;
pub mod pre_compact;
pub mod reconcile_graph;
pub mod user_prompt;
