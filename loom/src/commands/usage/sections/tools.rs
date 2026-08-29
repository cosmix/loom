//! Detects request shapes that repeatedly resend context for a single tool
//! action, a signal that batching or a narrower worker brief may save tokens.

use crate::commands::usage::transcript::Transcript;

use super::fmt::{format_u64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct ToolShape {
    pub assistant_requests: usize,
    pub single_tool_requests: usize,
    pub single_tool_share: f64,
    pub runs: usize,
    pub longest_run: usize,
    pub requests_in_runs: usize,
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
    ToolShape {
        assistant_requests,
        single_tool_requests,
        single_tool_share: ratio(single_tool_requests, assistant_requests),
        runs,
        longest_run,
        requests_in_runs,
    }
}

pub fn render(shape: &ToolShape) {
    heading("Tool request shape");
    if shape.assistant_requests == 0 {
        no_data("assistant requests");
        return;
    }
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
