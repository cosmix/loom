//! Stage state transitions and dependency triggering
//!
//! This module handles:
//! - Transitioning stages to new statuses
//! - Triggering dependent stages when dependencies are satisfied
//! - Loading and saving stage state to/from `.work/stages/` markdown files

mod persistence;
mod serialization;
mod state;

#[cfg(test)]
mod tests;

// Public API
pub use persistence::{list_all_stages, load_stage, save_stage};
pub use serialization::{parse_stage_from_markdown, serialize_stage_to_markdown};
pub use state::{transition_stage, trigger_dependents};
