//! Neutralizing a record read from disk before the status feed carries it.
//!
//! The current copy and the archive are plain files a stage agent could plant or edit, and
//! the record is drawn by the terminal overview and the web dialog. Every string is flattened
//! to one printable line (no control or bidi characters), and every count is clamped to what a
//! JavaScript number holds exactly, so a planted `1e300` cannot reach a width calculation.

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::build::serde_name;
use super::record::{ModelStats, PlanStatsRecord, StageStats};
use crate::context::untrusted::inline_safe;
use crate::models::stage::{StageStatus, StageType};
use crate::tokens::plan_totals::{CLAUDE_LANE, CODEX_LANE};
use crate::tokens::TokenUsage;

/// The largest integer a JavaScript number holds exactly (2^53 - 1), the limit the status
/// feed already applies to its other counts (`status/data/sanitize_token_flow.rs`).
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// Whether every enumerated string of `record` is one the build writes: a stage type and
/// status that round-trip through their serde names, and a lane that is `claude` or `codex`.
/// The web schema is strict about exactly these, so a record failing here (a planted file)
/// is refused rather than shown.
pub(super) fn has_known_names(record: &PlanStatsRecord) -> bool {
    record.stages.iter().all(|stage| {
        is_serde_name::<StageType>(&stage.stage_type) && is_serde_name::<StageStatus>(&stage.status)
    }) && record
        .models
        .iter()
        .all(|model| model.lane == CLAUDE_LANE || model.lane == CODEX_LANE)
}

/// `name` is the canonical serialized name of a `T` (an alias or a stray string is not).
fn is_serde_name<T: Serialize + DeserializeOwned>(name: &str) -> bool {
    serde_json::from_value::<T>(serde_json::Value::String(name.to_owned()))
        .is_ok_and(|value| serde_name(&value) == name)
}

/// Flatten every string of `record` and clamp every count into `0..=2^53-1`, in place.
pub(super) fn sanitize(record: &mut PlanStatsRecord) {
    flatten(&mut record.plan_id);
    flatten(&mut record.plan_name);
    record.wall_secs = clamp_secs(record.wall_secs);
    record.execution_secs = clamp_secs(record.execution_secs);
    clamp_usage(&mut record.totals);
    record.stages.iter_mut().for_each(sanitize_stage);
    record.models.iter_mut().for_each(sanitize_model);
}

fn sanitize_stage(stage: &mut StageStats) {
    flatten(&mut stage.id);
    flatten(&mut stage.name);
    flatten(&mut stage.stage_type);
    flatten(&mut stage.status);
    stage.model.iter_mut().for_each(flatten);
    stage.effort.iter_mut().for_each(flatten);
    stage.execution_secs = clamp_secs(stage.execution_secs);
    clamp_usage(&mut stage.tokens);
}

fn sanitize_model(model: &mut ModelStats) {
    flatten(&mut model.model);
    model.effort.iter_mut().for_each(flatten);
    flatten(&mut model.lane);
    clamp_usage(&mut model.tokens);
}

fn flatten(value: &mut String) {
    *value = inline_safe(value);
}

fn clamp_secs(secs: i64) -> i64 {
    secs.clamp(0, MAX_SAFE_INTEGER as i64)
}

fn clamp_usage(usage: &mut TokenUsage) {
    for field in [
        &mut usage.input,
        &mut usage.cache_write,
        &mut usage.cache_read,
        &mut usage.output,
    ] {
        *field = (*field).min(MAX_SAFE_INTEGER);
    }
}
