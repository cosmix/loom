//! Stage state manipulation
//! Usage: loom stage <id> [complete|block|reset|ready]

mod complete;
mod session;
mod skip_retry;
mod state;

#[cfg(test)]
mod tests;

// Re-export public API
pub use complete::complete;
pub use skip_retry::{retry, skip};
pub use state::{block, hold, ready, release, reset, resume_from_waiting, waiting};
