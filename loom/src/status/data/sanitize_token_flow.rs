//! The token flow as the status payload takes it from `flow.json`.
//!
//! The file is written by the tracker, but a planted copy reads the same, so
//! the collector treats it as untrusted, like every other free-form value of
//! the payload (see the parent module). Its strings are flattened. Its numbers
//! are bounded to what the web client reads: the dashboard schema rejects an
//! integer past JavaScript's safe range and then drops the whole status frame,
//! and an unbounded series would swell every frame a second.

use super::flatten;
use crate::tokens::model::{
    TokenFlow, TokenUsage, FLOW_STALE_SECS, MAX_MODEL_ROWS, PLAN_SERIES_LEN, STAGE_SERIES_LEN,
};

/// The largest cache hit percent.
const MAX_HIT_PERCENT: u8 = 100;

/// The largest integer JavaScript holds exactly: 2^53 - 1.
pub(crate) const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// Whether a flow observed at `observed_at` is recent enough to show at `now`.
/// A future timestamp is as untrustworthy as an old one, and the subtraction
/// saturates, so no `observed_at` can overflow it.
pub(crate) fn flow_is_fresh(observed_at: i64, now: i64) -> bool {
    (0..=FLOW_STALE_SECS).contains(&now.saturating_sub(observed_at))
}

/// Flatten every string of `flow` and bound every number and series, in place.
/// `minutes_left` is a `u32`, so it is inside the safe range already.
pub(crate) fn sanitize_token_flow(flow: &mut TokenFlow) {
    flow.observed_at = clamp_signed(flow.observed_at);
    flow.bucket_secs = clamp_signed(flow.bucket_secs);
    flow.unreadable_lines = clamp(flow.unreadable_lines);
    clamp_usage(&mut flow.plan.total);
    clamp_usage(&mut flow.plan.rate);
    bound_series(&mut flow.plan.series, PLAN_SERIES_LEN);
    flow.models.truncate(MAX_MODEL_ROWS);
    for entry in &mut flow.models {
        flatten(&mut entry.model);
        entry.effort.iter_mut().for_each(flatten);
        // A blank effort would show as an empty label and share a key with `None`.
        entry.effort = entry.effort.take().filter(|effort| !effort.is_empty());
        clamp_usage(&mut entry.total);
    }
    for stage in &mut flow.stages {
        flatten(&mut stage.stage_id);
        clamp_usage(&mut stage.total);
        clamp_usage(&mut stage.rate);
        bound_series(&mut stage.series, STAGE_SERIES_LEN);
        bound_hit_series(&mut stage.hit_series, STAGE_SERIES_LEN);
        for agent in &mut stage.agents {
            flatten(&mut agent.id);
            agent.agent_type.iter_mut().for_each(flatten);
            agent.model.iter_mut().for_each(flatten);
            clamp_usage(&mut agent.total);
            clamp_usage(&mut agent.rate);
            bound_series(&mut agent.series, STAGE_SERIES_LEN);
        }
    }
}

fn clamp(value: u64) -> u64 {
    value.min(MAX_SAFE_INTEGER)
}

fn clamp_signed(value: i64) -> i64 {
    let limit = MAX_SAFE_INTEGER as i64;
    value.clamp(-limit, limit)
}

fn clamp_usage(usage: &mut TokenUsage) {
    for field in [
        &mut usage.input,
        &mut usage.cache_write,
        &mut usage.cache_read,
        &mut usage.output,
    ] {
        *field = clamp(*field);
    }
}

/// Cuts `series` (oldest first) to its newest `len` values and bounds each.
fn bound_series(series: &mut Vec<u64>, len: usize) {
    series.drain(..series.len().saturating_sub(len));
    series.iter_mut().for_each(|value| *value = clamp(*value));
}

/// Cuts `series` (oldest first) to its newest `len` values and caps each
/// percent.
fn bound_hit_series(series: &mut Vec<Option<u8>>, len: usize) {
    series.drain(..series.len().saturating_sub(len));
    for value in series.iter_mut().flatten() {
        *value = (*value).min(MAX_HIT_PERCENT);
    }
}

#[cfg(test)]
#[path = "sanitize_token_flow_tests.rs"]
mod tests;
