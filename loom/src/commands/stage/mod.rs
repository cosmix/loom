//! Stage state manipulation
//! Usage: loom stage <id> [complete|block|reset|merge|retry|verify|...]

pub(crate) mod acceptance_runner;
mod check_acceptance;
mod complete;
mod criteria_runner;
mod dispute_criteria;
mod human_review;
mod knowledge_complete;
mod merge;
mod merge_resolver;
mod output;
mod progressive_complete;
pub(crate) mod recover;
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
pub use human_review::human_review;
pub use merge::merge;
pub use output::{
    get as output_get, list as output_list, remove as output_remove, set as output_set,
};
pub use skip_retry::{retry, skip};
pub use state::{block, hold, release, reset, resume_from_waiting, waiting};
pub use verify::verify;
