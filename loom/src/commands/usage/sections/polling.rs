//! Finds loops that spend request turns asking an unchanged question, so they
//! can be replaced by a wait, event, or a less frequent check.

use std::collections::{BTreeMap, BTreeSet};

use crate::commands::usage::transcript::{Request, ToolUse, Transcript};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;
use crate::context::untrusted::inline_safe;

use super::fmt::{format_f64, format_u64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct Polling {
    pub repeated_commands: Vec<RepeatedCommand>,
    pub sleep_invocations: usize,
    pub total_sleep_seconds: f64,
    pub observation: ObservationTokens,
}

/// Requests whose every tool use only observes: waiting on a subagent,
/// checking status, sleeping, or repeating a command. Their resident tokens
/// are what a wait or an event would save.
#[derive(Debug, serde::Serialize, Default)]
pub struct ObservationTokens {
    pub requests: usize,
    pub resident_input_tokens: u64,
    pub output_tokens: u64,
    pub by_kind: Vec<ObservationKind>,
}

#[derive(Debug, serde::Serialize)]
pub struct ObservationKind {
    pub kind: String,
    pub requests: usize,
    pub resident_input_tokens: u64,
}

/// A Bash command seen this often in one transcript counts as repeated.
const REPEAT_THRESHOLD: usize = 3;

#[derive(Debug, serde::Serialize)]
pub struct RepeatedCommand {
    pub transcript: String,
    pub command: String,
    pub count: usize,
}

pub fn build(transcripts: &[Transcript]) -> Polling {
    let repeated_commands = repeated_commands(transcripts);
    let (sleep_invocations, total_sleep_seconds) = sleep_stats(transcripts);
    Polling {
        repeated_commands,
        sleep_invocations,
        total_sleep_seconds,
        observation: observation_tokens(transcripts),
    }
}

fn has_data(polling: &Polling) -> bool {
    !polling.repeated_commands.is_empty()
        || polling.sleep_invocations > 0
        || polling.observation.requests > 0
}

pub fn render(polling: &Polling) {
    heading("Polling and sleeps");
    if !has_data(polling) {
        no_data("polling");
        return;
    }
    row("sleep invocations", polling.sleep_invocations);
    row(
        "total sleep seconds",
        format_f64(polling.total_sleep_seconds),
    );
    let observation = &polling.observation;
    row("observation-only requests", observation.requests);
    row(
        "observation resident tokens",
        format_u64(observation.resident_input_tokens),
    );
    for kind in &observation.by_kind {
        println!(
            "    {}: {} requests, {} resident tokens",
            kind.kind,
            kind.requests,
            format_u64(kind.resident_input_tokens)
        );
    }
    if !polling.repeated_commands.is_empty() {
        println!("  repeated Bash commands:");
        for command in &polling.repeated_commands {
            println!(
                "    {} [{}]: {}",
                inline_safe(&command.transcript),
                command.count,
                inline_safe(&command.command)
            );
        }
    }
}

fn repeated_commands(transcripts: &[Transcript]) -> Vec<RepeatedCommand> {
    let mut repeated = Vec::new();
    for transcript in transcripts {
        let mut counts = BTreeMap::<String, usize>::new();
        for command in bash_commands(transcript) {
            *counts.entry(command).or_default() += 1;
        }
        for (command, count) in counts
            .into_iter()
            .filter(|(_, count)| *count >= REPEAT_THRESHOLD)
        {
            repeated.push(RepeatedCommand {
                transcript: transcript_name(transcript),
                command: truncate_command(&command),
                count,
            });
        }
    }
    repeated.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.transcript.cmp(&right.transcript))
            .then_with(|| left.command.cmp(&right.command))
    });
    repeated
}

fn observation_tokens(transcripts: &[Transcript]) -> ObservationTokens {
    let mut total = ObservationTokens::default();
    let mut kinds = BTreeMap::<&'static str, (usize, u64)>::new();
    for transcript in transcripts {
        let repeated = repeated_set(transcript);
        for request in transcript.requests() {
            let Some(kind) = observation_kind(request, &repeated) else {
                continue;
            };
            let resident = request.usage.resident();
            total.requests += 1;
            total.resident_input_tokens += resident;
            total.output_tokens += request.usage.output;
            let entry = kinds.entry(kind).or_default();
            entry.0 += 1;
            entry.1 += resident;
        }
    }
    total.by_kind = kinds
        .into_iter()
        .map(|(kind, (requests, resident))| ObservationKind {
            kind: kind.to_owned(),
            requests,
            resident_input_tokens: resident,
        })
        .collect();
    total.by_kind.sort_by(|left, right| {
        right
            .resident_input_tokens
            .cmp(&left.resident_input_tokens)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    total
}

fn repeated_set(transcript: &Transcript) -> BTreeSet<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for command in bash_commands(transcript) {
        *counts.entry(command).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= REPEAT_THRESHOLD)
        .map(|(command, _)| command)
        .collect()
}

/// The request's kind (that of its first tool use) when every tool use
/// observes; `None` for a request that also acts or has no tool use.
fn observation_kind(request: &Request, repeated: &BTreeSet<String>) -> Option<&'static str> {
    if request.model == SYNTHETIC_MODEL {
        return None;
    }
    let mut kinds = request
        .tool_uses
        .iter()
        .map(|tool| tool_kind(tool, repeated));
    let first = kinds.next()??;
    kinds.all(|kind| kind.is_some()).then_some(first)
}

fn tool_kind(tool: &ToolUse, repeated: &BTreeSet<String>) -> Option<&'static str> {
    match tool.name.as_str() {
        "Monitor" => Some("monitor"),
        "ScheduleWakeup" => Some("wakeup"),
        "Bash" => {
            let command = tool.input.get("command")?.as_str()?;
            let trimmed = command.trim_start();
            if [
                "loom subagents watch",
                "loom subagents wait",
                "loom subagents list",
            ]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
            {
                Some("subagents-wait")
            } else if ["loom status", "loom request status", "git status"]
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
            {
                Some("status")
            } else if sleep_seconds(command).is_some() {
                Some("sleep")
            } else {
                repeated.contains(command).then_some("repeated")
            }
        }
        _ => None,
    }
}

fn sleep_stats(transcripts: &[Transcript]) -> (usize, f64) {
    let mut invocations = 0;
    let mut seconds = 0.0;
    for transcript in transcripts {
        for command in bash_commands(transcript) {
            if let Some(duration) = sleep_seconds(&command) {
                invocations += 1;
                seconds += duration;
            }
        }
    }
    (invocations, seconds)
}

fn bash_commands(transcript: &Transcript) -> Vec<String> {
    transcript
        .requests()
        .flat_map(|request| request.tool_uses.iter())
        .filter_map(|tool| {
            (tool.name == "Bash")
                .then(|| {
                    tool.input
                        .get("command")
                        .and_then(serde_json::Value::as_str)
                })?
                .map(str::to_owned)
        })
        .collect()
}

fn sleep_seconds(command: &str) -> Option<f64> {
    command
        .trim_start()
        .strip_prefix("sleep ")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn transcript_name(transcript: &Transcript) -> String {
    transcript.agent_id.clone().unwrap_or_else(|| {
        transcript
            .path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_owned)
            .unwrap_or_else(|| "unknown".to_owned())
    })
}

fn truncate_command(command: &str) -> String {
    const LIMIT: usize = 96;
    if command.chars().count() <= LIMIT {
        return command.to_owned();
    }
    let prefix = command
        .chars()
        .take(LIMIT.saturating_sub(1))
        .collect::<String>();
    format!("{prefix}…")
}

#[cfg(test)]
#[path = "polling_tests.rs"]
mod tests;
