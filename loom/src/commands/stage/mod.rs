//! Stage state manipulation
//! Usage: loom stage <id> [complete|block|reset|ready|merge-complete|recover]

mod complete;
mod merge_complete;
mod output;
mod recover;
mod session;
mod skip_retry;
mod state;

#[cfg(test)]
mod tests;

// Re-export public API
pub use complete::complete;
pub use merge_complete::merge_complete;
pub use output::{
    get as output_get, list as output_list, remove as output_remove, set as output_set,
};
pub use recover::recover;
pub use skip_retry::{retry, skip};
pub use state::{block, hold, ready, release, reset, resume_from_waiting, waiting};
