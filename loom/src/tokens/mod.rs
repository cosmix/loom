//! Token flow: per-stage, per-agent token usage read from Claude transcripts
//! and Codex rollouts, bucketed into rate series and published as `flow.json`.

pub(crate) mod accumulate;
mod anchor;
mod checkpoint;
pub mod claude_line;
pub mod codex_line;
mod discover;
mod flow_build;
mod messages;
pub mod model;
mod pass_memo;
pub mod plan_totals;
mod planner;
#[cfg(test)]
mod planner_contract_tests;
pub mod quota_left;
mod quota_points;
#[cfg(test)]
mod rewrite_contract_tests;
#[cfg(test)]
mod shed_contract_tests;
mod source_read;
pub(crate) mod tail;
#[cfg(test)]
mod tests_fixture;
pub mod tracker;
mod tracker_handle;

pub use model::*;
pub use planner::{planner_pass, spawn_planner_tokens, PlannerGroup};
pub use tracker::{
    final_scan, scan_pass, spawn_token_tracker, FinalScan, CHECKPOINT_FILE, TRACKER_INTERVAL_SECS,
};
pub use tracker_handle::TokenTracker;
