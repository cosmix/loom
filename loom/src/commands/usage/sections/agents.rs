//! Makes small, expensive workers visible before their work is normalized into
//! aggregate totals. Loom's hook ledger is authoritative for agent type, the
//! Claude Code sidecar fills a missing ledger row, and prompt inference is a
//! best-effort fallback for transcripts with neither.

use std::collections::BTreeMap;

use crate::commands::usage::accounting::UsageSummary;
use crate::commands::usage::transcript::{Scope, Transcript};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;
use crate::context::untrusted::inline_safe;

use super::fmt::{format_f64, format_u64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct AgentReport {
    pub subagent_transcripts: usize,
    pub agent_type_ledger_matches: usize,
    pub agent_type_sidecar_matches: usize,
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
    pub requested_model: String,
    pub model: String,
    pub requests: usize,
    #[serde(flatten)]
    pub(crate) summary: UsageSummary,
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
        .filter(|item| item.scope == Scope::Subagent && item.requests().next().is_some())
        .collect::<Vec<_>>();
    let tiny_subagents = tiny_subagents(&subagents);
    let by_agent_model = by_agent_model(&subagents);
    let parent_model = parent_matches(&subagents, &parent_models);
    AgentReport {
        subagent_transcripts: subagents.len(),
        agent_type_ledger_matches: subagents
            .iter()
            .filter(|transcript| {
                transcript.agent_type.is_some() && !transcript.agent_type_from_sidecar
            })
            .count(),
        agent_type_sidecar_matches: subagents
            .iter()
            .filter(|transcript| transcript.agent_type_from_sidecar)
            .count(),
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
    row(
        "agent type ledger",
        format!(
            "{}/{} transcripts matched",
            report.agent_type_ledger_matches, report.subagent_transcripts
        ),
    );
    row(
        "agent type sidecar",
        format!(
            "{}/{} transcripts matched",
            report.agent_type_sidecar_matches, report.subagent_transcripts
        ),
    );
    row("under-500-output subagents", report.tiny_subagents.len());
    for agent in &report.tiny_subagents {
        println!("{}", tiny_line(agent));
    }
    if report.by_agent_model.is_empty() {
        row("agent type and model", "no data");
    } else {
        println!("  agent type and model:");
        for item in &report.by_agent_model {
            println!("{}", agent_model_line(item));
        }
    }
    let parent = &report.parent_model;
    println!(
        "  parent-model requests: same {} different {} unknown {} (same known {:.1}%)",
        parent.same, parent.different, parent.unknown, parent.same_share_of_known
    );
}

// Every identity field is transcript or ledger text, so each goes through
// `inline_safe` before it reaches the terminal.
fn tiny_line(agent: &TinySubagent) -> String {
    format!(
        "    {} [{} / {}]: output {} input {}",
        inline_safe(&agent.agent_id),
        inline_safe(&agent.agent_type),
        inline_safe(&agent.model),
        format_u64(agent.output),
        format_u64(agent.input)
    )
}

fn agent_model_line(item: &AgentModelRow) -> String {
    format!(
        "    {} / requested {} / {}: {} requests, input {}, output {}, S2 {}",
        inline_safe(&item.agent_type),
        inline_safe(&item.requested_model),
        inline_safe(&item.model),
        item.requests,
        format_u64(item.summary.usage.input),
        format_u64(item.summary.usage.output),
        format_f64(item.summary.accounting.s2)
    )
}

fn parent_models(transcripts: &[Transcript]) -> BTreeMap<String, String> {
    let mut models = BTreeMap::new();
    for transcript in transcripts.iter().filter(|item| item.scope == Scope::Main) {
        if let Some(request) = transcript
            .requests()
            .find(|request| request.model != SYNTHETIC_MODEL)
        {
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
                    .find(|item| item.model != SYNTHETIC_MODEL)
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
    let mut grouped = BTreeMap::<(String, String, String), AgentModelRow>::new();
    for transcript in subagents {
        let kind = agent_type(transcript);
        let requested = transcript
            .requested_model
            .clone()
            .unwrap_or_else(|| "unknown".to_owned());
        for request in transcript.requests() {
            if request.model == SYNTHETIC_MODEL {
                continue;
            }
            let row = grouped
                .entry((kind.clone(), requested.clone(), request.model.clone()))
                .or_insert_with(|| AgentModelRow {
                    agent_type: kind.clone(),
                    requested_model: requested.clone(),
                    model: request.model.clone(),
                    requests: 0,
                    summary: UsageSummary::default(),
                });
            row.requests += 1;
            row.summary.add(&request.usage);
        }
    }
    let mut rows = grouped.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .summary
            .accounting
            .s2
            .total_cmp(&left.summary.accounting.s2)
            .then_with(|| left.agent_type.cmp(&right.agent_type))
            .then_with(|| left.requested_model.cmp(&right.requested_model))
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
            if request.model == SYNTHETIC_MODEL {
                continue;
            }
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

/// Known prompt identities; excludes the ordinary word `Explore` to avoid
/// false matches in task prose.
const KNOWN_AGENT_TYPES: [&str; 7] = [
    "loom-senior-software-engineer",
    "loom-software-engineer",
    "loom-codex-forwarder",
    "loom-code-reviewer",
    "general-purpose",
    "loom-advisor",
    "loom-mechanical-engineer",
];

/// Infer a type from delimited prompt mentions when no joined type exists.
/// Ignore cost annotations and return `unknown` for ambiguous matches.
fn agent_type(transcript: &Transcript) -> String {
    if let Some(agent_type) = transcript.agent_type.as_deref() {
        return agent_type.to_owned();
    }
    let prompt = transcript
        .first_user_entry
        .as_ref()
        .map_or("", |entry| entry.text.as_str());
    let matches: Vec<&str> = KNOWN_AGENT_TYPES
        .into_iter()
        .filter(|name| mentions_agent_type(prompt, name))
        .collect();
    match matches.as_slice() {
        [name] => (*name).to_owned(),
        _ => "unknown".to_owned(),
    }
}

/// True when `name` occurs in `prompt` as a delimited identity mention: the
/// character immediately before and after the match must not be part of a
/// longer identifier (alphanumeric, `-`, or `_`), and the match must not be
/// immediately followed by (optional whitespace then) `=` -- the shape of
/// CLAUDE.md's "(name = tier)" cost annotation.
fn mentions_agent_type(prompt: &str, name: &str) -> bool {
    let is_identifier_char = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    prompt.match_indices(name).any(|(start, matched)| {
        let before_ok = prompt[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_identifier_char(c));
        let end = start + matched.len();
        let after_ok = prompt[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_identifier_char(c));
        let is_cost_annotation = prompt[end..].trim_start().starts_with('=');
        before_ok && after_ok && !is_cost_annotation
    })
}

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;
