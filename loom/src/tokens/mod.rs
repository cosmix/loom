//! Token flow: per-stage, per-agent token usage read from Claude transcripts
//! and Codex rollouts, bucketed into rate series and published as `flow.json`.

mod accumulate;
mod checkpoint;
pub mod claude_line;
pub mod codex_line;
mod discover;
mod flow_build;
pub mod model;
mod pass_memo;
pub mod quota_left;
mod quota_points;
mod tail;
#[cfg(test)]
mod tests_fixture;
pub mod tracker;

pub use model::*;
pub use tracker::{scan_pass, spawn_token_tracker, CHECKPOINT_FILE, TRACKER_INTERVAL_SECS};
