//! Makes small, expensive workers visible before their work is normalized into
//! aggregate totals. Agent type is inferred from the first user message by
//! known prompt names, because transcripts do not record a type explicitly.

use std::collections::BTreeMap;

use crate::commands::usage::accounting::Accounting;
use crate::commands::usage::transcript::{Scope, Transcript};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;
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
    let mut grouped = BTreeMap::<(String, String), AgentModelRow>::new();
    for transcript in subagents {
        let kind = agent_type(transcript);
        for request in transcript.requests() {
            if request.model == SYNTHETIC_MODEL {
                continue;
            }
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

/// Names inferred from a subagent's spawn prompt. `agent_type` collects every
/// match and returns a name only when exactly one survives, so this list's
/// order carries no meaning -- it need not be sorted or otherwise ordered.
///
/// Deliberately excludes `Explore`: unlike every other candidate here, it is
/// an ordinary English word (as in "Explore the tree first"), so it is the
/// one name likely to appear as a false-positive match in unrelated task
/// prose. That false match is worse than merely mislabeling one Explore
/// spawn as `unknown` -- co-occurring with a second, genuine match collapses
/// an otherwise-unambiguous identification to `unknown` too, per the
/// disambiguation rule below. There is no realistic textual context to
/// require instead (e.g. a `subagent_type=` neighbourhood): the whole reason
/// this module falls back to prompt text is that the real spawn type is
/// never echoed into the subagent's own received prompt in the first place.
const KNOWN_AGENT_TYPES: [&str; 6] = [
    "loom-senior-software-engineer",
    "loom-software-engineer",
    "loom-codex-forwarder",
    "loom-code-reviewer",
    "general-purpose",
    "loom-advisor",
];

/// Infer a subagent's type from its spawn prompt. The transcript format
/// carries no explicit type field, so this is a best-effort textual
/// fallback: `commands::subagents::ledger::agent_type` reads the hook-written
/// `starts.jsonl`/`spawns.jsonl` ledgers and is the authoritative source when
/// it applies, but that module is scoped to `commands::subagents` and this
/// report has no `.work/` root threaded through `build` to reach it.
///
/// Plain substring matching against a fixed-order list used to pick
/// whichever known name happened to be checked first, which broke two ways:
/// `loom-software-engineer` was checked before `loom-senior-software-engineer`,
/// and CLAUDE.md's own Rule 6c coordinator preamble writes
/// "(loom-software-engineer = sonnet)" into virtually every coordinator
/// prompt regardless of the coordinator's real type, so that boilerplate
/// alone made every senior-engineer spawn get reported as the sonnet tier.
/// This version (1) never counts a name immediately followed by `=` as an
/// identity mention -- that shape is CLAUDE.md's own cost-tier annotation,
/// naming every known type without declaring the current transcript's own
/// identity; (2) requires the match be delimited, not embedded inside a
/// longer identifier; (3) refuses to guess when more than one distinct name
/// survives those two filters, since a wrong label here is worse than an
/// absent one.
fn agent_type(transcript: &Transcript) -> String {
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

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;
