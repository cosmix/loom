//! What each agent type starts with: the first request's resident context and
//! the attachment kinds already inside it.

use std::collections::BTreeMap;

use super::super::fmt::percentile;
use super::analyze::Startup;

/// Kinds kept per agent type, largest first.
const MAX_KINDS: usize = 8;

#[derive(Debug, serde::Serialize)]
pub struct StartupRow {
    pub agent_type: String,
    pub transcripts: u64,
    pub median_resident: u64,
    pub kinds: Vec<StartupKind>,
}

#[derive(Debug, serde::Serialize)]
pub struct StartupKind {
    pub kind: String,
    pub median_tokens: u64,
}

/// One row per agent type, busiest first. A kind's median covers only the
/// transcripts that start with it.
pub(super) fn by_agent(startups: Vec<(String, Startup)>) -> Vec<StartupRow> {
    let mut grouped = BTreeMap::<String, Vec<Startup>>::new();
    for (agent_type, startup) in startups {
        grouped.entry(agent_type).or_default().push(startup);
    }
    let mut rows: Vec<StartupRow> = grouped
        .into_iter()
        .map(|(agent_type, group)| row(agent_type, group))
        .collect();
    rows.sort_by(|left, right| {
        right
            .transcripts
            .cmp(&left.transcripts)
            .then_with(|| left.agent_type.cmp(&right.agent_type))
    });
    rows
}

fn row(agent_type: String, group: Vec<Startup>) -> StartupRow {
    let mut residents: Vec<u64> = group.iter().map(|startup| startup.resident).collect();
    let mut tokens = BTreeMap::<String, Vec<u64>>::new();
    for startup in &group {
        for (kind, chars) in &startup.kind_chars {
            tokens
                .entry(kind.clone())
                .or_default()
                .push(*chars as u64 / 4);
        }
    }
    let mut kinds: Vec<StartupKind> = tokens
        .into_iter()
        .map(|(kind, mut values)| StartupKind {
            kind,
            median_tokens: percentile(&mut values, 0.5),
        })
        .collect();
    kinds.sort_by(|left, right| {
        right
            .median_tokens
            .cmp(&left.median_tokens)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    kinds.truncate(MAX_KINDS);
    StartupRow {
        agent_type,
        transcripts: group.len() as u64,
        median_resident: percentile(&mut residents, 0.5),
        kinds,
    }
}
