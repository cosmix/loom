//! Builds the published [`TokenFlow`] from the tracker's per-agent state, and
//! fits its serialized form under [`MAX_FLOW_BYTES`].

use super::accumulate::{AgentState, StageAgents};
use super::discover::{CODEX_PREFIX, MAIN_PREFIX};
use super::model::{
    AgentFlow, AgentKind, ModelUsage, PlanFlow, StageFlow, TokenFlow, TokenUsage, ACTIVE_BUCKETS,
    BUCKET_SECS, MAX_FLOW_BYTES, MAX_MODEL_ROWS, OTHER_MODEL, PLAN_SERIES_LEN, RATE_BUCKETS,
    STAGE_SERIES_LEN,
};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap};

/// Characters of the session uuid or thread id a short agent id keeps.
const SHORT_ID_CHARS: usize = 8;

type Buckets = BTreeMap<i64, TokenUsage>;

/// The flow at `now` over every stage's agents, stages in id order, with
/// `quota_left` empty.
pub(super) fn build_flow(stages: &BTreeMap<String, StageAgents>, now: i64) -> TokenFlow {
    let now_bucket = now.div_euclid(BUCKET_SECS);
    let mut total = TokenUsage::default();
    let mut buckets = Buckets::new();
    let mut unreadable_lines = 0u64;
    let mut flows = Vec::with_capacity(stages.len());
    for (stage_id, agents) in stages {
        for agent in agents.values() {
            unreadable_lines = unreadable_lines.saturating_add(agent.unreadable);
        }
        let (flow, stage_buckets) = stage_flow(stage_id, agents, now);
        total.add(&flow.total);
        merge(&mut buckets, &stage_buckets);
        flows.push(flow);
    }
    TokenFlow {
        observed_at: now,
        bucket_secs: BUCKET_SECS,
        plan: PlanFlow {
            total,
            rate: rate(&buckets, now, now_bucket),
            series: series(&buckets, now_bucket, PLAN_SERIES_LEN),
        },
        stages: flows,
        models: model_totals(stages),
        quota_left: Vec::new(),
        unreadable_lines,
    }
}

/// `flow` serialized compactly within [`MAX_FLOW_BYTES`]. Over the cap, the
/// series of every inactive agent is emptied (its totals stay) and the flow is
/// measured again; `None` when it is still over.
pub(super) fn fit_flow(flow: &mut TokenFlow) -> Result<Option<String>> {
    let json = serde_json::to_string(flow).context("serializing the token flow")?;
    if json.len() as u64 <= MAX_FLOW_BYTES {
        return Ok(Some(json));
    }
    let agents = flow.stages.iter_mut().flat_map(|stage| &mut stage.agents);
    for agent in agents.filter(|agent| !agent.active) {
        agent.series.clear();
    }
    let json = serde_json::to_string(flow).context("serializing the token flow")?;
    Ok((json.len() as u64 <= MAX_FLOW_BYTES).then_some(json))
}

/// One stage's flow and its summed buckets.
fn stage_flow(stage_id: &str, agents: &StageAgents, now: i64) -> (StageFlow, Buckets) {
    let now_bucket = now.div_euclid(BUCKET_SECS);
    let mut total = TokenUsage::default();
    let mut buckets = Buckets::new();
    let mut flows = Vec::with_capacity(agents.len());
    for (id, agent) in ordered(agents) {
        total.add(&agent.total);
        merge(&mut buckets, &agent.buckets);
        flows.push(agent_flow(id, agent, now));
    }
    let flow = StageFlow {
        stage_id: stage_id.to_owned(),
        total,
        rate: rate(&buckets, now, now_bucket),
        series: series(&buckets, now_bucket, STAGE_SERIES_LEN),
        hit_series: hit_series(&buckets, now_bucket, STAGE_SERIES_LEN),
        agents: flows,
    };
    (flow, buckets)
}

fn agent_flow(id: String, agent: &AgentState, now: i64) -> AgentFlow {
    let now_bucket = now.div_euclid(BUCKET_SECS);
    let series = series(&agent.buckets, now_bucket, STAGE_SERIES_LEN);
    let active = series
        .iter()
        .rev()
        .take(ACTIVE_BUCKETS)
        .any(|fresh| *fresh > 0);
    AgentFlow {
        id,
        kind: agent.kind,
        agent_type: agent.agent_type.clone(),
        model: agent.model.clone(),
        active,
        total: agent.total,
        rate: rate(&agent.buckets, now, now_bucket),
        series,
    }
}

/// The stage's agents in display order (main agents, then subagents, then
/// Codex, each by first-seen time) with their published ids: the short form,
/// unless two agents of the stage share it.
fn ordered(agents: &StageAgents) -> Vec<(String, &AgentState)> {
    let mut list: Vec<(&str, &AgentState)> = agents
        .iter()
        .map(|(key, agent)| (key.as_str(), agent))
        .collect();
    list.sort_by_key(|(key, agent)| {
        let first_seen = agent.first_seen.unwrap_or(i64::MAX);
        (kind_rank(agent.kind), first_seen, *key)
    });
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for (key, agent) in &list {
        *uses.entry(short_id(key, agent.kind)).or_default() += 1;
    }
    list.iter()
        .map(|(key, agent)| {
            let short = short_id(key, agent.kind);
            let shared = uses.get(short).is_some_and(|count| *count > 1);
            let id = if shared { *key } else { short };
            (id.to_owned(), *agent)
        })
        .collect()
}

fn kind_rank(kind: AgentKind) -> u8 {
    match kind {
        AgentKind::Main => 0,
        AgentKind::Subagent => 1,
        AgentKind::Codex => 2,
    }
}

/// `main-` or `codex-` plus the first [`SHORT_ID_CHARS`] of the key's uuid;
/// a subagent's id is its key.
fn short_id(key: &str, kind: AgentKind) -> &str {
    let prefix = match kind {
        AgentKind::Main => MAIN_PREFIX,
        AgentKind::Codex => CODEX_PREFIX,
        AgentKind::Subagent => return key,
    };
    key.get(..prefix.len() + SHORT_ID_CHARS).unwrap_or(key)
}

fn merge(into: &mut Buckets, from: &Buckets) {
    for (bucket, usage) in from {
        into.entry(*bucket).or_default().add(usage);
    }
}

/// Fresh tokens per bucket, oldest first, ending at `now_bucket`.
fn series(buckets: &Buckets, now_bucket: i64, len: usize) -> Vec<u64> {
    let first = now_bucket - (len as i64 - 1);
    (first..=now_bucket)
        .map(|bucket| buckets.get(&bucket).map_or(0, TokenUsage::fresh))
        .collect()
}

/// Cache hit percent per bucket, oldest first, ending at `now_bucket`: `None`
/// for a bucket without input-side tokens, else the share of `cache_read` in
/// `cache_read + input + cache_write`, rounded half up.
fn hit_series(buckets: &Buckets, now_bucket: i64, len: usize) -> Vec<Option<u8>> {
    let first = now_bucket - (len as i64 - 1);
    (first..=now_bucket)
        .map(|bucket| buckets.get(&bucket).and_then(hit_percent))
        .collect()
}

fn hit_percent(usage: &TokenUsage) -> Option<u8> {
    let denominator = usage
        .cache_read
        .saturating_add(usage.input)
        .saturating_add(usage.cache_write);
    if denominator == 0 {
        return None;
    }
    let doubled = usage
        .cache_read
        .saturating_mul(200)
        .saturating_add(denominator);
    let percent = (doubled / denominator.saturating_mul(2)).min(100);
    Some(percent as u8)
}

/// Usage per minute over the last [`RATE_BUCKETS`] buckets up to `now`, integer
/// division per field. The window runs from the start of the oldest bucket to
/// `now`, so its span is 271 to 300 s and the rate does not drop when a new
/// bucket starts.
fn rate(buckets: &Buckets, now: i64, now_bucket: i64) -> TokenUsage {
    let first = now_bucket - (RATE_BUCKETS as i64 - 1);
    let mut sum = TokenUsage::default();
    for usage in buckets.range(first..=now_bucket).map(|(_, usage)| usage) {
        sum.add(usage);
    }
    let elapsed = now - now_bucket * BUCKET_SECS;
    let span = ((RATE_BUCKETS as i64 - 1) * BUCKET_SECS + elapsed + 1) as u64;
    let per_minute = |field: u64| field.saturating_mul(60) / span;
    TokenUsage {
        input: per_minute(sum.input),
        cache_write: per_minute(sum.cache_write),
        cache_read: per_minute(sum.cache_read),
        output: per_minute(sum.output),
    }
}

/// Plan-wide usage per (model, effort), largest fresh total first, then model,
/// then effort with `None` last. Past [`MAX_MODEL_ROWS`] entries, the tail and
/// any existing `OTHER_MODEL` entry merge into one `OTHER_MODEL` entry placed
/// last, so the entries still sum to the plan total.
fn model_totals(stages: &BTreeMap<String, StageAgents>) -> Vec<ModelUsage> {
    // A map, not a scan of the merged entries: a checkpoint is not capped by
    // `MAX_MODEL_KEYS`, so the distinct keys can be many.
    let mut totals: BTreeMap<(&str, Option<&str>), TokenUsage> = BTreeMap::new();
    for entry in stages
        .values()
        .flat_map(|agents| agents.values())
        .flat_map(|agent| &agent.by_model)
    {
        totals
            .entry((entry.model.as_str(), entry.effort.as_deref()))
            .or_default()
            .add(&entry.total);
    }
    let mut merged: Vec<ModelUsage> = totals
        .into_iter()
        .map(|((model, effort), total)| ModelUsage {
            model: model.to_owned(),
            effort: effort.map(str::to_owned),
            total,
        })
        .collect();
    merged.sort_by(|a, b| {
        let effort_key = |entry: &ModelUsage| (entry.effort.is_none(), entry.effort.clone());
        (std::cmp::Reverse(a.total.fresh()), &a.model, effort_key(a)).cmp(&(
            std::cmp::Reverse(b.total.fresh()),
            &b.model,
            effort_key(b),
        ))
    });
    if merged.len() > MAX_MODEL_ROWS {
        fold_tail(&mut merged);
    }
    merged
}

/// Merges the entries from position `MAX_MODEL_ROWS - 1` on, and any
/// `OTHER_MODEL` entry before it, into one `OTHER_MODEL` entry placed last.
fn fold_tail(sorted: &mut Vec<ModelUsage>) {
    let tail = sorted.split_off(MAX_MODEL_ROWS - 1);
    let mut other = ModelUsage {
        model: OTHER_MODEL.to_owned(),
        effort: None,
        total: TokenUsage::default(),
    };
    for entry in &tail {
        other.total.add(&entry.total);
    }
    if let Some(at) = sorted
        .iter()
        .position(|entry| entry.model == OTHER_MODEL && entry.effort.is_none())
    {
        other.total.add(&sorted.remove(at).total);
    }
    sorted.push(other);
}

#[cfg(test)]
#[path = "flow_build_tests.rs"]
mod tests;
