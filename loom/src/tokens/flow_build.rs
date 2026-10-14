//! Builds the published [`TokenFlow`] from the tracker's per-agent state, and
//! fits its serialized form under [`MAX_FLOW_BYTES`].

use super::accumulate::{AgentState, StageAgents};
use super::discover::{CODEX_PREFIX, MAIN_PREFIX};
use super::model::{
    AgentFlow, AgentKind, PlanFlow, StageFlow, TokenFlow, TokenUsage, ACTIVE_BUCKETS, BUCKET_SECS,
    MAX_FLOW_BYTES, PLAN_SERIES_LEN, RATE_BUCKETS, STAGE_SERIES_LEN,
};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap};

/// Characters of the session uuid or thread id a short agent id keeps.
const SHORT_ID_CHARS: usize = 8;
/// Minutes the rate window spans: the divisor of a rate.
const RATE_MINUTES: u64 = (RATE_BUCKETS as u64 * BUCKET_SECS as u64) / 60;

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
        let (flow, stage_buckets) = stage_flow(stage_id, agents, now_bucket);
        total.add(&flow.total);
        merge(&mut buckets, &stage_buckets);
        flows.push(flow);
    }
    TokenFlow {
        observed_at: now,
        bucket_secs: BUCKET_SECS,
        plan: PlanFlow {
            total,
            rate: rate(&buckets, now_bucket),
            series: series(&buckets, now_bucket, PLAN_SERIES_LEN),
        },
        stages: flows,
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
fn stage_flow(stage_id: &str, agents: &StageAgents, now_bucket: i64) -> (StageFlow, Buckets) {
    let mut total = TokenUsage::default();
    let mut buckets = Buckets::new();
    let mut flows = Vec::with_capacity(agents.len());
    for (id, agent) in ordered(agents) {
        total.add(&agent.total);
        merge(&mut buckets, &agent.buckets);
        flows.push(agent_flow(id, agent, now_bucket));
    }
    let flow = StageFlow {
        stage_id: stage_id.to_owned(),
        total,
        rate: rate(&buckets, now_bucket),
        series: series(&buckets, now_bucket, STAGE_SERIES_LEN),
        agents: flows,
    };
    (flow, buckets)
}

fn agent_flow(id: String, agent: &AgentState, now_bucket: i64) -> AgentFlow {
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
        rate: rate(&agent.buckets, now_bucket),
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

/// Usage per minute over the last [`RATE_BUCKETS`] buckets, integer division
/// per field.
fn rate(buckets: &Buckets, now_bucket: i64) -> TokenUsage {
    let first = now_bucket - (RATE_BUCKETS as i64 - 1);
    let mut sum = TokenUsage::default();
    for usage in buckets.range(first..=now_bucket).map(|(_, usage)| usage) {
        sum.add(usage);
    }
    TokenUsage {
        input: sum.input / RATE_MINUTES,
        cache_write: sum.cache_write / RATE_MINUTES,
        cache_read: sum.cache_read / RATE_MINUTES,
        output: sum.output / RATE_MINUTES,
    }
}

#[cfg(test)]
#[path = "flow_build_tests.rs"]
mod tests;
