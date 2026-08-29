//! Finds loops that spend request turns asking an unchanged question, so they
//! can be replaced by a wait, event, or a less frequent check.

use std::collections::BTreeMap;

use crate::commands::usage::transcript::Transcript;
use crate::context::untrusted::inline_safe;

use super::fmt::{format_f64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct Polling {
    pub repeated_commands: Vec<RepeatedCommand>,
    pub sleep_invocations: usize,
    pub total_sleep_seconds: f64,
}

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
    }
}

pub fn render(polling: &Polling) {
    heading("Polling and sleeps");
    if polling.repeated_commands.is_empty() && polling.sleep_invocations == 0 {
        no_data("polling");
        return;
    }
    row("sleep invocations", polling.sleep_invocations);
    row(
        "total sleep seconds",
        format_f64(polling.total_sleep_seconds),
    );
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
        for (command, count) in counts.into_iter().filter(|(_, count)| *count >= 3) {
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
