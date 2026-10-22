//! Measures how much a session spends reading before it acts, in requests and
//! tokens, for main sessions and subagents. Orientation ends at the first
//! request that edits, commits or spawns.

use crate::commands::usage::transcript::{Request, Scope, Transcript};

use super::fmt::percentile;

/// Subagent types whose last act is a recorded verdict, so they have no
/// orientation phase to measure.
const READ_ONLY_TYPES: [&str; 3] = ["loom-code-reviewer", "loom-advisor", "Explore"];

#[derive(Debug, serde::Serialize, Default)]
pub struct Orientation {
    pub main: OrientationScope,
    pub subagent: OrientationScope,
    pub skipped_read_only: usize,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct OrientationScope {
    pub samples: usize,
    pub without_end: usize,
    pub median_requests: usize,
    pub p90_requests: usize,
    pub median_tokens: u64,
    pub p90_tokens: u64,
}

#[derive(Default)]
struct Samples {
    requests: Vec<usize>,
    tokens: Vec<u64>,
    without_end: usize,
}

pub fn build(transcripts: &[Transcript]) -> Orientation {
    let mut main = Samples::default();
    let mut subagent = Samples::default();
    let mut skipped_read_only = 0;
    for transcript in transcripts {
        if transcript.requests().next().is_none() {
            continue;
        }
        let samples = match transcript.scope {
            Scope::Main => &mut main,
            Scope::Subagent if is_read_only(transcript) => {
                skipped_read_only += 1;
                continue;
            }
            Scope::Subagent => &mut subagent,
        };
        match orientation_of(transcript) {
            Some((requests, tokens)) => {
                samples.requests.push(requests);
                samples.tokens.push(tokens);
            }
            None => samples.without_end += 1,
        }
    }
    Orientation {
        main: scope(main),
        subagent: scope(subagent),
        skipped_read_only,
    }
}

fn scope(mut samples: Samples) -> OrientationScope {
    OrientationScope {
        samples: samples.requests.len(),
        without_end: samples.without_end,
        median_requests: percentile(&mut samples.requests, 0.5),
        p90_requests: percentile(&mut samples.requests, 0.9),
        median_tokens: percentile(&mut samples.tokens, 0.5),
        p90_tokens: percentile(&mut samples.tokens, 0.9),
    }
}

fn is_read_only(transcript: &Transcript) -> bool {
    transcript
        .agent_type
        .as_deref()
        .is_some_and(|kind| READ_ONLY_TYPES.contains(&kind))
}

/// Requests and tokens before the first ending request, or `None` when the
/// transcript never edits, commits or spawns.
fn orientation_of(transcript: &Transcript) -> Option<(usize, u64)> {
    let mut tokens = 0;
    for (count, request) in transcript.requests().enumerate() {
        if ends_orientation(request) {
            return Some((count, tokens));
        }
        tokens += request.usage.resident() + request.usage.output;
    }
    None
}

fn ends_orientation(request: &Request) -> bool {
    request
        .tool_uses
        .iter()
        .any(|tool| match tool.name.as_str() {
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "Agent" | "Task" => true,
            "Bash" => tool
                .input
                .get("command")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|command| {
                    command.contains("git commit") || command.contains("loom stage commit")
                }),
            _ => false,
        })
}
