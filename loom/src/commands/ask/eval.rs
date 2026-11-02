//! `loom ask --eval`: run a cases file against one backend, one case at a
//! time, and report accuracy, worker tokens, latency and what reading the
//! files directly would have cost.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{ensure, Context, Result};
use serde::Deserialize;

use super::{ask_documents, payload, run, AskArgs, AskRequest, WorkerReply};
use crate::context::untrusted::inline_safe;
use crate::fs::safe_read::read_to_string_bounded;

/// The share of cases that must pass for exit 0.
pub(crate) const ACCURACY_BAR: f64 = 0.90;
/// Largest cases file read.
const MAX_CASES_BYTES: usize = 1024 * 1024;

/// One question with the strings a correct answer must and must not hold.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvalCase {
    pub id: String,
    pub question: String,
    /// Relative to the cases file's directory.
    pub paths: Vec<String>,
    pub must_contain: Vec<String>,
    #[serde(default)]
    pub must_not_contain: Vec<String>,
}

/// Parse a JSON array of cases, refusing an empty list, an empty `id`,
/// `question`, `paths` or `must_contain`, an empty match string, and
/// duplicate ids.
pub(crate) fn parse_cases(text: &str) -> Result<Vec<EvalCase>> {
    let cases: Vec<EvalCase> =
        serde_json::from_str(text).context("the cases file is not a JSON array of cases")?;
    ensure!(!cases.is_empty(), "the cases file holds no cases");
    let mut ids = HashSet::new();
    for case in &cases {
        ensure!(!case.id.trim().is_empty(), "a case has an empty id");
        let id = inline_safe(&case.id);
        ensure!(
            !case.question.trim().is_empty(),
            "case {id} has an empty question"
        );
        ensure!(!case.paths.is_empty(), "case {id} lists no paths");
        ensure!(
            !case.must_contain.is_empty(),
            "case {id} has no must_contain"
        );
        let mut strings = case.must_contain.iter().chain(&case.must_not_contain);
        ensure!(
            strings.all(|s| !s.is_empty()),
            "case {id} has an empty match string"
        );
        ensure!(ids.insert(case.id.as_str()), "duplicate case id {id}");
    }
    Ok(cases)
}

/// Run every case and print the report. Returns 0 when accuracy reaches
/// [`ACCURACY_BAR`], else 1. Stops before the next case once interrupted.
pub(crate) fn run(project_root: &Path, args: &AskArgs) -> Result<i32> {
    let cases_arg = args.eval.as_deref().context("--eval names no cases file")?;
    let (_, relative) = payload::locate(project_root, cases_arg)?;
    let text = read_to_string_bounded(project_root, &relative, MAX_CASES_BYTES)
        .with_context(|| format!("cannot read the cases file {}", relative.display()))?;
    let cases = parse_cases(&text)?;
    let base = project_root.join(relative.parent().unwrap_or_else(|| Path::new("")));
    let mut tally = Tally::default();
    for case in &cases {
        if run::interrupted() {
            break;
        }
        let (outcome, chars) = run_case(project_root, &base, case, args);
        println!("{}", tally.record(case, outcome, chars));
    }
    Ok(tally.report(cases.len()))
}

/// Ask one case's question; also returns the characters of its files.
fn run_case(
    root: &Path,
    base: &Path,
    case: &EvalCase,
    args: &AskArgs,
) -> (Result<WorkerReply>, usize) {
    let paths: Vec<PathBuf> = case.paths.iter().map(|path| base.join(path)).collect();
    let documents = match payload::read_documents(root, &paths) {
        Ok(documents) => documents,
        Err(error) => return (Err(error), 0),
    };
    let chars = documents.iter().map(|doc| doc.text.chars().count()).sum();
    let request = AskRequest {
        question: case.question.clone(),
        backend: args.backend,
        model: args.model.clone(),
        timeout: Duration::from_secs(args.timeout_secs),
    };
    (ask_documents(&documents, &request), chars)
}

/// The first `must_contain` string missing from `answer`, or the first
/// `must_not_contain` string present, ignoring ASCII case.
fn first_miss(case: &EvalCase, answer: &str) -> Option<String> {
    let answer = answer.to_ascii_lowercase();
    let holds = |needle: &String| answer.contains(&needle.to_ascii_lowercase());
    if let Some(missing) = case.must_contain.iter().find(|s| !holds(s)) {
        return Some(format!("missing \"{}\"", inline_safe(missing)));
    }
    case.must_not_contain
        .iter()
        .find(|s| holds(s))
        .map(|forbidden| format!("forbidden \"{}\"", inline_safe(forbidden)))
}

#[derive(Default)]
struct Tally {
    passed: usize,
    input: u64,
    cache_read: u64,
    cache_write: u64,
    output: u64,
    latencies: Vec<u64>,
    direct_tokens: usize,
}

impl Tally {
    /// Count one case and return its report line.
    fn record(&mut self, case: &EvalCase, outcome: Result<WorkerReply>, chars: usize) -> String {
        self.direct_tokens += chars / 4;
        let id = inline_safe(&case.id);
        let reply = match outcome {
            Ok(reply) => reply,
            Err(error) => return format!("{id}: fail ({})", inline_safe(&format!("{error:#}"))),
        };
        self.input += reply.input_tokens;
        self.cache_read += reply.cache_read_tokens;
        self.cache_write += reply.cache_write_tokens;
        self.output += reply.output_tokens;
        self.latencies.push(reply.latency_ms);
        let verdict = match first_miss(case, &reply.answer) {
            Some(miss) => format!("fail ({miss})"),
            None => {
                self.passed += 1;
                "pass".to_owned()
            }
        };
        format!(
            "{id}: {verdict} tokens input {} cache_read {} cache_write {} output {} latency {} ms",
            reply.input_tokens,
            reply.cache_read_tokens,
            reply.cache_write_tokens,
            reply.output_tokens,
            reply.latency_ms
        )
    }

    /// Print the summary over `total` cases and return the exit code.
    fn report(mut self, total: usize) -> i32 {
        let accuracy = self.passed as f64 / total.max(1) as f64;
        self.latencies.sort_unstable();
        println!("accuracy: {accuracy:.2}");
        println!(
            "worker tokens: input {} cache_read {} cache_write {} output {}",
            self.input, self.cache_read, self.cache_write, self.output
        );
        println!(
            "latency p50: {} p90: {}",
            percentile(&self.latencies, 50),
            percentile(&self.latencies, 90)
        );
        println!("direct tokens estimate: {}", self.direct_tokens);
        i32::from(accuracy < ACCURACY_BAR)
    }
}

/// Nearest-rank percentile of sorted values; 0 when there are none.
fn percentile(sorted: &[u64], percent: usize) -> u64 {
    let rank = (sorted.len() * percent).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().unwrap_or(0)
}

#[cfg(test)]
#[path = "eval_tests.rs"]
mod eval_tests;
