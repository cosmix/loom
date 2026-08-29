//! Tests whether subagent prompting and handoff behaviour match the intended
//! lifecycle, so prompt rules can be simplified when they do not change work.
//! A brief is a heuristic: 500+ characters remain after the recognised
//! preamble; a `=== TASK ===` delimiter ends that preamble when it is present.

use std::collections::BTreeMap;

use crate::commands::usage::transcript::{Entry, Scope, Transcript};

use super::fmt::{heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct Lifecycle {
    pub subagent_transcripts: usize,
    pub classes: Vec<PromptClass>,
    pub brief: BriefPresence,
    pub requests_before_first_edit: RequestDelay,
    pub post_turn_messages: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct PromptClass {
    pub class: String,
    pub transcripts: usize,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct BriefPresence {
    pub with_brief: usize,
    pub without_brief: usize,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct RequestDelay {
    pub samples: usize,
    pub without_edit: usize,
    pub median: usize,
    pub p90: usize,
}

pub fn build(transcripts: &[Transcript]) -> Lifecycle {
    let subagents = transcripts
        .iter()
        .filter(|item| item.scope == Scope::Subagent)
        .collect::<Vec<_>>();
    let classes = class_distribution(&subagents);
    let brief = brief_presence(&subagents);
    let requests_before_first_edit = request_delays(&subagents);
    let post_turn_messages = subagents.iter().map(|item| messages_after_turn(item)).sum();
    Lifecycle {
        subagent_transcripts: subagents.len(),
        classes,
        brief,
        requests_before_first_edit,
        post_turn_messages,
    }
}

pub fn render(lifecycle: &Lifecycle) {
    heading("Subagent lifecycle");
    if lifecycle.subagent_transcripts == 0 {
        no_data("subagent lifecycle");
        return;
    }
    println!("  spawn prompt classes:");
    for class in &lifecycle.classes {
        println!("    {}: {}", class.class, class.transcripts);
    }
    row(
        "brief present (500+ chars after preamble)",
        lifecycle.brief.with_brief,
    );
    row("brief absent", lifecycle.brief.without_brief);
    let delay = &lifecycle.requests_before_first_edit;
    println!(
        "  requests before first edit: median {} p90 {} ({} edited, {} never edited)",
        delay.median, delay.p90, delay.samples, delay.without_edit
    );
    row(
        "messages after first assistant turn",
        lifecycle.post_turn_messages,
    );
}

fn class_distribution(subagents: &[&Transcript]) -> Vec<PromptClass> {
    let mut counts = BTreeMap::<String, usize>::new();
    for transcript in subagents {
        *counts
            .entry(prompt_class(first_message(transcript)))
            .or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(class, transcripts)| PromptClass { class, transcripts })
        .collect()
}

fn brief_presence(subagents: &[&Transcript]) -> BriefPresence {
    let mut result = BriefPresence::default();
    for transcript in subagents {
        let prompt = first_message(transcript);
        if prompt_after_preamble(prompt).chars().count() >= 500 {
            result.with_brief += 1;
        } else {
            result.without_brief += 1;
        }
    }
    result
}

fn request_delays(subagents: &[&Transcript]) -> RequestDelay {
    let mut delays = subagents
        .iter()
        .filter_map(|item| requests_before_edit(item))
        .collect::<Vec<_>>();
    let samples = delays.len();
    let without_edit = subagents.len().saturating_sub(samples);
    let median = percentile(&mut delays.clone(), 0.5);
    let p90 = percentile(&mut delays, 0.9);
    RequestDelay {
        samples,
        without_edit,
        median,
        p90,
    }
}

fn first_message(transcript: &Transcript) -> &str {
    transcript
        .first_user_entry
        .as_ref()
        .map_or("", |entry| entry.text.as_str())
}

fn prompt_class(prompt: &str) -> String {
    let class = if prompt.contains("LOOM-CODEX-FORWARD-ONLY") {
        "codex-forwarder"
    } else if prompt.contains("COORDINATOR ROLE") {
        "coordinator preamble"
    } else if prompt.contains("WORKER RESTRICTIONS") {
        "worker preamble"
    } else if prompt.contains("SUBAGENT RESTRICTIONS") {
        "rule-5 preamble"
    } else {
        "none"
    };
    class.to_owned()
}

fn prompt_after_preamble(prompt: &str) -> &str {
    let marker = [
        "LOOM-CODEX-FORWARD-ONLY",
        "COORDINATOR ROLE",
        "WORKER RESTRICTIONS",
        "SUBAGENT RESTRICTIONS",
    ]
    .into_iter()
    .find(|marker| prompt.contains(marker));
    match marker {
        None => prompt,
        Some(marker) => prompt.split_once("=== TASK ===").map_or_else(
            || prompt.split_once(marker).map_or(prompt, |(_, rest)| rest),
            |(_, rest)| rest,
        ),
    }
}

fn requests_before_edit(transcript: &Transcript) -> Option<usize> {
    for (count, request) in transcript.requests().enumerate() {
        if request.tool_uses.iter().any(|tool| is_edit(&tool.name)) {
            return Some(count);
        }
    }
    None
}

fn messages_after_turn(transcript: &Transcript) -> usize {
    let first_assistant = transcript
        .entries
        .iter()
        .position(|entry| matches!(entry, Entry::Assistant(_)));
    first_assistant.map_or(0, |index| {
        transcript.entries[index + 1..]
            .iter()
            .filter(|entry| matches!(entry, Entry::User(user) if user.tool_use_id.is_none()))
            .count()
    })
}

fn is_edit(name: &str) -> bool {
    matches!(name, "Edit" | "Write" | "MultiEdit" | "NotebookEdit")
}

fn percentile(values: &mut [usize], fraction: f64) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[((values.len() as f64 * fraction).ceil() as usize).saturating_sub(1)]
}
