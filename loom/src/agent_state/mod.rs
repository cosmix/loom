//! Subagent-state rules shared by `loom subagents`, which reads a whole
//! transcript file, and the planner session bridge, which reads one line at a
//! time: structural progress, the stall rule table and background task ids.

pub mod background_id;
pub mod progress;
pub mod stall;

pub use background_id::background_task_id;
pub use progress::{AgentProgress, SubagentState, DEFAULT_DONE_DEBOUNCE_SECS};
pub use stall::stall_reason;
