//! Makes small, expensive workers visible before their work is normalized into
//! aggregate totals. Agent type is inferred from the first user message by
//! known prompt names, because transcripts do not record a type explicitly.

use std::collections::BTreeMap;

use crate::commands::usage::accounting::Accounting;
use crate::commands::usage::transcript::{Scope, Transcript};
use crate::context::untrusted::inline_safe;

use super::fmt::{format_f64, format_u64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct AgentReport {
    pub subagent_transcripts: usize,
    pub tiny_subagents: Vec<TinySubagent>,
    pub by_agent_model: Vec<AgentModelRow>,
    pub parent_model: ParentModelMatch,
}

#[derive(Debug, serde::Serialize)]
pub struct TinySubagent {
    pub agent_type: String,
    pub agent_id: String,
    pub model: String,
    pub output: u64,
    pub input: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct AgentModelRow {
    pub agent_type: String,
    pub model: String,
    pub requests: usize,
    pub fresh_input: u64,
    pub cache_creation: u64,
    pub cache_reads: u64,
    pub output: u64,
    pub cache_writes_5m: u64,
    pub cache_writes_1h: u64,
    pub s1: u64,
    pub s2: f64,
    pub s3: u64,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct ParentModelMatch {
    pub same: usize,
    pub different: usize,
    pub unknown: usize,
    pub same_share_of_known: f64,
}

pub fn build(transcripts: &[Transcript]) -> AgentReport {
    let parent_models = parent_models(transcripts);
    let subagents = transcripts
        .iter()
        .filter(|item| item.scope == Scope::Subagent)
        .collect::<Vec<_>>();
    let tiny_subagents = tiny_subagents(&subagents);
    let by_agent_model = by_agent_model(&subagents);
    let parent_model = parent_matches(&subagents, &parent_models);
    AgentReport {
        subagent_transcripts: subagents.len(),
        tiny_subagents,
        by_agent_model,
        parent_model,
    }
}

pub fn render(report: &AgentReport) {
    heading("Subagents");
    if report.subagent_transcripts == 0 {
        no_data("subagent transcripts");
        return;
    }
    row("subagent transcripts", report.subagent_transcripts);
    row("under-500-output subagents", report.tiny_subagents.len());
    for agent in &report.tiny_subagents {
        println!(
            "    {} [{} / {}]: output {} input {}",
            inline_safe(&agent.agent_id),
            agent.agent_type,
            inline_safe(&agent.model),
            format_u64(agent.output),
            format_u64(agent.input)
        );
    }
    if report.by_agent_model.is_empty() {
        row("agent type and model", "no data");
    } else {
        println!("  agent type and model:");
        for item in &report.by_agent_model {
            println!(
                "    {} / {}: {} requests, input {}, output {}, S2 {}",
                item.agent_type,
                inline_safe(&item.model),
                item.requests,
                format_u64(item.fresh_input),
                format_u64(item.output),
                format_f64(item.s2)
            );
        }
    }
    let parent = &report.parent_model;
    println!(
        "  parent-model requests: same {} different {} unknown {} (same known {:.1}%)",
        parent.same, parent.different, parent.unknown, parent.same_share_of_known
    );
}

fn parent_models(transcripts: &[Transcript]) -> BTreeMap<String, String> {
    let mut models = BTreeMap::new();
    for transcript in transcripts.iter().filter(|item| item.scope == Scope::Main) {
        if let Some(request) = transcript.requests().next() {
            models
                .entry(transcript.session_id.clone())
                .or_insert_with(|| request.model.clone());
        }
    }
    models
}

fn tiny_subagents(subagents: &[&Transcript]) -> Vec<TinySubagent> {
    let mut tiny = subagents
        .iter()
        .filter_map(|transcript| {
            let usage = transcript.total_usage();
            (usage.output < 500).then(|| TinySubagent {
                agent_type: agent_type(transcript),
                agent_id: transcript
                    .agent_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned()),
                model: transcript
                    .requests()
                    .next()
                    .map(|item| item.model.clone())
                    .unwrap_or_else(|| "(no requests)".to_owned()),
                output: usage.output,
                input: usage.input,
            })
        })
        .collect::<Vec<_>>();
    tiny.sort_by(|left, right| {
        right
            .input
            .cmp(&left.input)
            .then_with(|| left.agent_id.cmp(&right.agent_id))
    });
    tiny
}

fn by_agent_model(subagents: &[&Transcript]) -> Vec<AgentModelRow> {
    let mut grouped = BTreeMap::<(String, String), AgentModelRow>::new();
    for transcript in subagents {
        let kind = agent_type(transcript);
        for request in transcript.requests() {
            let row = grouped
                .entry((kind.clone(), request.model.clone()))
                .or_insert_with(|| AgentModelRow {
                    agent_type: kind.clone(),
                    model: request.model.clone(),
                    requests: 0,
                    fresh_input: 0,
                    cache_creation: 0,
                    cache_reads: 0,
                    output: 0,
                    cache_writes_5m: 0,
                    cache_writes_1h: 0,
                    s1: 0,
                    s2: 0.0,
                    s3: 0,
                });
            row.requests += 1;
            add_usage(row, request);
        }
    }
    let mut rows = grouped.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .s2
            .total_cmp(&left.s2)
            .then_with(|| left.agent_type.cmp(&right.agent_type))
            .then_with(|| left.model.cmp(&right.model))
    });
    rows
}

fn parent_matches(
    subagents: &[&Transcript],
    parents: &BTreeMap<String, String>,
) -> ParentModelMatch {
    let mut matches = ParentModelMatch::default();
    for transcript in subagents {
        for request in transcript.requests() {
            match parents.get(&transcript.session_id) {
                Some(model) if model == &request.model => matches.same += 1,
                Some(_) => matches.different += 1,
                None => matches.unknown += 1,
            }
        }
    }
    let known = matches.same + matches.different;
    matches.same_share_of_known = if known == 0 {
        0.0
    } else {
        matches.same as f64 * 100.0 / known as f64
    };
    matches
}

fn agent_type(transcript: &Transcript) -> String {
    let prompt = transcript
        .first_user_entry
        .as_ref()
        .map_or("", |entry| entry.text.as_str());
    for name in [
        "loom-software-engineer",
        "loom-senior-software-engineer",
        "loom-code-reviewer",
        "loom-codex-forwarder",
        "loom-advisor",
        "Explore",
        "general-purpose",
    ] {
        if prompt.contains(name) {
            return name.to_owned();
        }
    }
    "unknown".to_owned()
}

fn add_usage(row: &mut AgentModelRow, request: &crate::commands::usage::transcript::Request) {
    let usage = &request.usage;
    let accounting = Accounting::of(usage);
    row.fresh_input += usage.input;
    row.cache_creation += usage.cache_creation;
    row.cache_reads += usage.cache_read;
    row.output += usage.output;
    row.cache_writes_5m += usage.ephemeral_5m;
    row.cache_writes_1h += usage.ephemeral_1h;
    row.s1 += accounting.s1;
    row.s2 += accounting.s2;
    row.s3 += accounting.s3;
}
