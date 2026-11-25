//! Deriving a [`PlanStatsRecord`] from the persisted stages and the token totals.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::identity::{self, RunIdentity};
use super::record::{ModelStats, PlanStatsRecord, StageStats, MAX_RECORD_BYTES, SCHEMA_VERSION};
use super::sanitize::sanitize;
use crate::fs::work_dir::load_config;
use crate::models::stage::{Stage, StageStatus};
use crate::tokens::plan_totals::{plan_tokens, PlanTokens};
use crate::verify::transitions::list_all_stages;

/// Name shown when the config holds none.
const FALLBACK_PLAN_NAME: &str = "Plan";

/// Build the record of the initialized run in `work_dir` from its persisted stages.
///
/// Wall time starts at the earliest stage start (first phase span or persisted `started_at`)
/// and ends at the latest completion, phase end or skip transition; a phase can end after
/// `completed_at` (a merge wait), and a skip sets no `completed_at`, so a skipped stage
/// counts through its last update. A plan in which no stage ever ran has the
/// initialization time for both ends and no duration. Every count is a saturating sum.
pub fn build(work_dir: &Path) -> Result<PlanStatsRecord> {
    let run = identity::ensure(work_dir)?;
    let plan_name = load_config(work_dir)?
        .and_then(|config| config.get_plan_str("plan_name").map(str::to_string))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| FALLBACK_PLAN_NAME.to_string());
    let stages = list_all_stages(work_dir)?;
    let ids: BTreeSet<String> = stages.iter().map(|stage| stage.id.clone()).collect();
    let tokens = plan_tokens(work_dir, &ids).context("cannot total the plan's tokens")?;
    assemble(&run, plan_name, stages, &tokens)
}

/// The record of `run` from its stages and token totals, with no file access.
pub(super) fn assemble(
    run: &RunIdentity,
    plan_name: String,
    mut stages: Vec<Stage>,
    tokens: &PlanTokens,
) -> Result<PlanStatsRecord> {
    stages.sort_by(|a, b| start_order(a).cmp(&start_order(b)));
    let (started_at, ended_at) = plan_span(&stages, run.initialized_at);
    let mut record = PlanStatsRecord {
        schema_version: SCHEMA_VERSION,
        plan_id: run.plan_id(),
        plan_name,
        started_at,
        ended_at,
        wall_secs: (ended_at - started_at).num_seconds().max(0),
        execution_secs: stages
            .iter()
            .fold(0, |sum, s| sum.saturating_add(execution_secs(s))),
        totals: tokens.totals,
        stages: stages
            .iter()
            .map(|stage| stage_stats(stage, tokens))
            .collect(),
        models: model_stats(tokens),
    };
    sanitize(&mut record);
    fit_to_limit(&mut record)?;
    Ok(record)
}

/// Stages with a start sort by it, the rest after them; ties break on the id.
fn start_order(stage: &Stage) -> (bool, Option<DateTime<Utc>>, &str) {
    let start = stage_start(stage);
    (start.is_none(), start, stage.id.as_str())
}

/// When the stage first worked: its first phase span, else its persisted start.
fn stage_start(stage: &Stage) -> Option<DateTime<Utc>> {
    let phase = stage.phases.first().map(|span| span.started_at);
    match (phase, stage.started_at) {
        (Some(phase), Some(started)) => Some(phase.min(started)),
        (phase, started) => phase.or(started),
    }
}

fn plan_span(stages: &[Stage], fallback: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let Some(start) = stages.iter().filter_map(stage_start).min() else {
        return (fallback, fallback);
    };
    let end = stages
        .iter()
        .filter_map(stage_end)
        .max()
        .map_or(start, |end| end.max(start));
    (start, end)
}

/// When the stage finished: the later of its completion and its last closed phase span
/// (a merge held after completion keeps the stage waiting past `completed_at`), or for a
/// skipped stage the skip transition (the stage file's last update).
fn stage_end(stage: &Stage) -> Option<DateTime<Utc>> {
    if stage.status == StageStatus::Skipped {
        return Some(stage.updated_at);
    }
    let phase_end = stage.phases.iter().filter_map(|span| span.ended_at).max();
    stage.completed_at.max(phase_end)
}

fn execution_secs(stage: &Stage) -> i64 {
    stage.execution_secs.unwrap_or(0).max(0)
}

fn stage_stats(stage: &Stage, tokens: &PlanTokens) -> StageStats {
    StageStats {
        id: stage.id.clone(),
        name: stage.name.clone(),
        stage_type: serde_name(&stage.stage_type),
        status: serde_name(&stage.status),
        started_at: stage.started_at,
        completed_at: stage.completed_at,
        execution_secs: execution_secs(stage),
        model: stage.model.clone(),
        effort: stage.reasoning_effort.clone(),
        phases: stage.phases.clone(),
        tokens: tokens.per_stage.get(&stage.id).copied().unwrap_or_default(),
    }
}

fn model_stats(tokens: &PlanTokens) -> Vec<ModelStats> {
    tokens
        .models
        .iter()
        .map(|usage| ModelStats {
            model: usage.model.clone(),
            effort: usage.effort.clone(),
            lane: usage.lane.clone(),
            agents: usage.agents,
            tokens: usage.tokens,
        })
        .collect()
}

/// The serialized name of a unit enum value, e.g. `needs-adjudication`.
pub(super) fn serde_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Keep the encoded record within [`MAX_RECORD_BYTES`] by dropping the phase spans of the
/// stage with the most of them, one stage at a time. Totals, durations and the stage rows
/// stay; only a timeline's detail is lost, and only for a plan of extraordinary size.
fn fit_to_limit(record: &mut PlanStatsRecord) -> Result<()> {
    while record.to_json()?.len() > MAX_RECORD_BYTES {
        let Some(stage) = record
            .stages
            .iter_mut()
            .filter(|stage| !stage.phases.is_empty())
            .max_by_key(|stage| stage.phases.len())
        else {
            bail!("the plan stats record exceeds {MAX_RECORD_BYTES} bytes without phase spans");
        };
        stage.phases.clear();
    }
    Ok(())
}
