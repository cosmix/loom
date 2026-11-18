//! Subagent-state rules shared by `loom subagents`, which reads a whole
//! transcript file, and the planner session bridge, which reads one line at a
//! time: structural progress, the stall rule table, background task ids and the safe-id check. It
//! is a leaf: nothing here imports a caller's module.

pub mod background_id;
pub mod progress;
pub mod safe_id;
pub mod stall;

pub use background_id::background_task_id;
pub use progress::{AgentProgress, SubagentState, DEFAULT_DONE_DEBOUNCE_SECS};
pub use safe_id::is_safe_id;
pub use stall::stall_reason;
