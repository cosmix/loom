//! Memory command implementations for managing session memory journals.
//!
//! Commands:
//! - `loom memory note <text>` - Record a note
//! - `loom memory decision <text> [--context <ctx>]` - Record a decision
//! - `loom memory question <text>` - Record a question
//! - `loom memory query <search>` - Search memory entries
//! - `loom memory list [--session <id>]` - List memory entries
//! - `loom memory show [--session <id>]` - Show full memory journal
//! - `loom memory sessions` - List all memory journals
//! - `loom memory promote <type> <target> [--session <id>]` - Promote entries to knowledge

mod formatters;
mod handlers;

// Re-export all public command handlers
pub use handlers::decision;
pub use handlers::list;
pub use handlers::note;
pub use handlers::promote;
pub use handlers::query;
pub use handlers::question;
pub use handlers::sessions;
pub use handlers::show;
