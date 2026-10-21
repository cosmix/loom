//! Detects request shapes that repeatedly resend context for a single tool
//! action, a signal that batching or a narrower worker brief may save tokens.

use std::collections::BTreeMap;

use crate::commands::usage::transcript::{Entry, Transcript};

use crate::context::untrusted::inline_safe;

use super::fmt::{format_u64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct ToolShape {
    pub assistant_requests: usize,
    pub single_tool_requests: usize,
    pub single_tool_share: f64,
    pub runs: usize,
    pub longest_run: usize,
    pub requests_in_runs: usize,
    pub bash_output: BashOutput,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct BashOutput {
    pub calls: usize,
    pub output_bytes: u64,
    pub by_command: Vec<CommandOutput>,
}

#[derive(Debug, serde::Serialize)]
pub struct CommandOutput {
    pub command: String,
    pub calls: usize,
    pub output_bytes: u64,
    pub max_bytes: u64,
}

pub fn build(transcripts: &[Transcript]) -> ToolShape {
    let assistant_requests = transcripts.iter().map(|item| item.requests().count()).sum();
    let single_tool_requests = transcripts
        .iter()
        .flat_map(Transcript::requests)
        .filter(|request| request.tool_uses.len() == 1)
        .count();
    let (runs, longest_run, requests_in_runs) = transcripts
        .iter()
        .map(run_stats)
        .fold((0, 0, 0), |total, next| {
            (total.0 + next.0, total.1.max(next.1), total.2 + next.2)
        });
    let mut bash_totals = BTreeMap::new();
    for transcript in transcripts {
        add_entries(&mut bash_totals, &transcript.entries);
    }
    ToolShape {
        assistant_requests,
        single_tool_requests,
        single_tool_share: ratio(single_tool_requests, assistant_requests),
        runs,
        longest_run,
        requests_in_runs,
        bash_output: bash_output(bash_totals),
    }
}

pub fn render(shape: &ToolShape) {
    heading("Tool request shape");
    if shape.assistant_requests == 0 {
        no_data("assistant requests");
    } else {
        row(
            "one-tool requests",
            format_u64(shape.single_tool_requests as u64),
        );
        row("one-tool share", format!("{:.1}%", shape.single_tool_share));
        row("runs of 3+", shape.runs);
        row("longest run", shape.longest_run);
        row(
            "requests inside runs",
            format_u64(shape.requests_in_runs as u64),
        );
    }
    heading("Bash output by command");
    row("bash results", format_u64(shape.bash_output.calls as u64));
    row(
        "bash output bytes",
        format_u64(shape.bash_output.output_bytes),
    );
    for command in &shape.bash_output.by_command {
        println!("{}", render_row(command));
    }
}

fn render_row(command: &CommandOutput) -> String {
    format!(
        "  {}: {} calls, {} bytes, max {}",
        inline_safe(&command.command),
        format_u64(command.calls as u64),
        format_u64(command.output_bytes),
        format_u64(command.max_bytes)
    )
}

fn add_entries(totals: &mut BTreeMap<String, (usize, u64, u64)>, entries: &[Entry]) {
    let mut commands = BTreeMap::new();
    for entry in entries {
        if let Entry::Assistant(request) = entry {
            for tool in &request.tool_uses {
                if tool.name == "Bash" {
                    let command = tool
                        .input
                        .get("command")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    commands.insert(tool.id.clone(), command_key(command));
                }
            }
        }
    }
    for entry in entries {
        let Entry::User(user) = entry else {
            continue;
        };
        let Some(tool_use_id) = &user.tool_use_id else {
            continue;
        };
        let Some(command) = commands.get(tool_use_id) else {
            continue;
        };
        let bytes = user.text.len() as u64;
        let total = totals.entry(command.clone()).or_default();
        total.0 += 1;
        total.1 += bytes;
        total.2 = total.2.max(bytes);
    }
}

fn bash_output(totals: BTreeMap<String, (usize, u64, u64)>) -> BashOutput {
    let (calls, output_bytes) = totals.values().fold((0, 0), |total, value| {
        (total.0 + value.0, total.1 + value.1)
    });
    let mut by_command = totals
        .into_iter()
        .map(
            |(command, (calls, output_bytes, max_bytes))| CommandOutput {
                command,
                calls,
                output_bytes,
                max_bytes,
            },
        )
        .collect::<Vec<_>>();
    by_command.sort_by(|left, right| {
        right
            .output_bytes
            .cmp(&left.output_bytes)
            .then_with(|| left.command.cmp(&right.command))
    });
    by_command.truncate(20);
    BashOutput {
        calls,
        output_bytes,
        by_command,
    }
}

/// Longest command key kept; transcript text is untrusted and unbounded.
const MAX_KEY_CHARS: usize = 96;

fn command_key(command: &str) -> String {
    bounded_key(&raw_command_key(command))
}

fn bounded_key(key: &str) -> String {
    key.chars().take(MAX_KEY_CHARS).collect()
}

fn raw_command_key(command: &str) -> String {
    let words = command.split_whitespace().collect::<Vec<_>>();
    let first_program = words
        .iter()
        .position(|word| !is_assignment(word))
        .unwrap_or(words.len());
    let words = &words[first_program..];
    let words = if words.len() >= 3 && words[0] == "cd" && words[2] == "&&" {
        &words[3..]
    } else {
        words
    };
    let Some(program) = words.first() else {
        return "(empty)".to_owned();
    };
    let program = program.rsplit('/').next().unwrap_or(program);
    match words.get(1).filter(|next| !next.starts_with('-')) {
        Some(next) if is_subcommand(program) => format!("{program} {next}"),
        _ => program.to_owned(),
    }
}

fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut characters = name.chars();
    matches!(characters.next(), Some('A'..='Z' | 'a'..='z' | '_'))
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn is_subcommand(program: &str) -> bool {
    matches!(
        program,
        "cargo" | "git" | "loom" | "bun" | "bunx" | "npm" | "uv" | "rg" | "fd" | "jq"
    )
}

fn run_stats(transcript: &Transcript) -> (usize, usize, usize) {
    let mut result = (0, 0, 0);
    let mut run = 0;
    for request in transcript.requests() {
        if request.tool_uses.len() == 1 {
            run += 1;
        } else {
            finish_run(&mut result, &mut run);
        }
    }
    finish_run(&mut result, &mut run);
    result
}

fn finish_run(result: &mut (usize, usize, usize), run: &mut usize) {
    if *run >= 3 {
        result.0 += 1;
        result.1 = result.1.max(*run);
        result.2 += *run;
    }
    *run = 0;
}

fn ratio(value: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        value as f64 * 100.0 / total as f64
    }
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
