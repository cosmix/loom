//! A plan's token totals: per stage and per (model, effort, lane), read from
//! the aggregates the run's final scan left in memory, else from the
//! checkpoint on disk.

use super::accumulate::StageAgents;
use super::checkpoint::Checkpoint;
use super::model::{self, AgentKind, TokenUsage, TOKENS_DIR};
use anyhow::{bail, Context, Result};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The lane of an agent that runs on Codex.
pub const CODEX_LANE: &str = "codex";
/// The lane of every other agent: a stage's main session and its subagents.
pub const CLAUDE_LANE: &str = "claude";

/// Tokens used with one model at one reasoning effort on one lane, and how
/// many agents used tokens there.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelUsage {
    pub model: String,
    pub effort: Option<String>,
    /// [`CLAUDE_LANE`] or [`CODEX_LANE`].
    pub lane: String,
    pub agents: u32,
    pub tokens: TokenUsage,
}

/// The token totals of one plan's stages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanTokens {
    pub totals: TokenUsage,
    /// Every requested stage id, zero when it used no tokens.
    pub per_stage: BTreeMap<String, TokenUsage>,
    /// Largest `fresh + cache_read` first, then model, effort (`None` last)
    /// and lane.
    pub models: Vec<ModelUsage>,
}

/// What one agent used: its total and that total split by (model, effort),
/// including the `unknown` and `other` keys.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AgentTotals {
    pub(super) lane: &'static str,
    pub(super) total: TokenUsage,
    pub(super) by_model: Vec<model::ModelUsage>,
}

/// Agent totals by stage id.
pub(super) type StageTotals = BTreeMap<String, Vec<AgentTotals>>;

/// The aggregates of the last final scan of each work root, by the path the
/// scan was given. One entry per root a process finishes, so it stays small.
static FINAL_SCANS: OnceLock<Mutex<HashMap<PathBuf, StageTotals>>> = OnceLock::new();

fn final_scans() -> &'static Mutex<HashMap<PathBuf, StageTotals>> {
    FINAL_SCANS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lane(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Codex => CODEX_LANE,
        AgentKind::Main | AgentKind::Subagent => CLAUDE_LANE,
    }
}

/// The totals of every agent of `stages`.
fn capture(stages: &BTreeMap<String, StageAgents>) -> StageTotals {
    stages
        .iter()
        .map(|(stage_id, agents)| {
            let agents = agents
                .values()
                .map(|agent| AgentTotals {
                    lane: lane(agent.kind),
                    total: agent.total,
                    by_model: agent.by_model.clone(),
                })
                .collect();
            (stage_id.clone(), agents)
        })
        .collect()
}

/// Keeps `stages`, as a final scan of `work_root` left them, for
/// [`plan_tokens`]; replaces what an earlier final scan of that path kept.
pub(super) fn register(work_root: &Path, stages: &BTreeMap<String, StageAgents>) {
    let captured = capture(stages);
    let mut scans = final_scans()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    scans.insert(work_root.to_path_buf(), captured);
}

fn registered(work_root: &Path) -> Option<StageTotals> {
    final_scans()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(work_root)
        .cloned()
}

/// Aggregates only the given stage ids. Uses the in-memory aggregates
/// registered by the final scan when present for this work_root (the same
/// path the scan was given), else a bounded offline checkpoint load: a
/// missing tokens directory or checkpoint is a run that used no tokens, a
/// symlinked or unusable one is an error.
pub fn plan_tokens(work_root: &Path, stage_ids: &BTreeSet<String>) -> Result<PlanTokens> {
    let stages = match registered(work_root) {
        Some(stages) => stages,
        None => load_offline(work_root)?,
    };
    Ok(aggregate(&stages, stage_ids))
}

/// The agent totals of `<work_root>/tokens/checkpoint.json`, read no-follow
/// and bounded by the checkpoint's size cap.
fn load_offline(work_root: &Path) -> Result<StageTotals> {
    let dir = work_root.join(TOKENS_DIR);
    let metadata = match std::fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(StageTotals::new()),
        Err(error) => return Err(error).with_context(|| format!("inspecting {}", dir.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("token flow path is not a directory: {}", dir.display());
    }
    match Checkpoint::try_load(&dir) {
        Ok(Some(checkpoint)) => Ok(capture(&checkpoint.stages)),
        Ok(None) => Ok(StageTotals::new()),
        Err(reason) => bail!("cannot read the token {reason}"),
    }
}

/// The totals of the stages `stage_ids` names. An agent counts once in each
/// (model, effort, lane) row where it used tokens.
pub(super) fn aggregate(stages: &StageTotals, stage_ids: &BTreeSet<String>) -> PlanTokens {
    let mut per_stage: BTreeMap<String, TokenUsage> = stage_ids
        .iter()
        .map(|id| (id.clone(), TokenUsage::default()))
        .collect();
    let mut totals = TokenUsage::default();
    let mut rows = Rows::new();
    let kept = stages.iter().filter(|(id, _)| stage_ids.contains(*id));
    for (stage_id, agents) in kept {
        let stage_total = per_stage.entry(stage_id.clone()).or_default();
        for agent in agents {
            stage_total.add(&agent.total);
            totals.add(&agent.total);
            let used = agent
                .by_model
                .iter()
                .filter(|entry| entry.total != TokenUsage::default());
            for entry in used {
                let key = (entry.model.as_str(), entry.effort.as_deref(), agent.lane);
                let row = rows.entry(key).or_default();
                row.0 = row.0.saturating_add(1);
                row.1.add(&entry.total);
            }
        }
    }
    let models = sorted_rows(rows);
    PlanTokens {
        totals,
        per_stage,
        models,
    }
}

type Rows<'a> = BTreeMap<(&'a str, Option<&'a str>, &'a str), (u32, TokenUsage)>;

fn sorted_rows(rows: Rows<'_>) -> Vec<ModelUsage> {
    let mut models: Vec<ModelUsage> = rows
        .into_iter()
        .map(|((model, effort, lane), (agents, tokens))| ModelUsage {
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
            lane: lane.to_owned(),
            agents,
            tokens,
        })
        .collect();
    models.sort_by_cached_key(|row| {
        let all = row.tokens.fresh().saturating_add(row.tokens.cache_read);
        (
            Reverse(all),
            row.model.clone(),
            (row.effort.is_none(), row.effort.clone()),
            row.lane.clone(),
        )
    });
    models
}

#[cfg(test)]
#[path = "plan_totals_tests.rs"]
mod tests;
