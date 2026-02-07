//! Stage state manipulation
//! Usage: loom stage <id> [complete|block|reset|ready|merge-complete|recover|verify|check-acceptance]

pub(crate) mod acceptance_runner;
mod check_acceptance;
mod complete;
mod criteria_runner;
mod dispute_criteria;
mod knowledge_complete;
mod merge_complete;
mod output;
mod progressive_complete;
mod recover;
pub(crate) mod session;
mod skip_retry;
mod state;
mod verify;

#[cfg(test)]
mod tests;

// Re-export public API
pub use check_acceptance::check_acceptance;
pub use complete::complete;
pub use dispute_criteria::dispute_criteria;
pub use merge_complete::merge_complete;
pub use output::{
    get as output_get, list as output_list, remove as output_remove, set as output_set,
};
pub use recover::recover;
pub use skip_retry::{retry, skip};
pub use state::{block, hold, ready, release, reset, resume_from_waiting, waiting};
pub use verify::verify;
