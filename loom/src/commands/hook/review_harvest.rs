//! `loom hook review-harvest`: record a `loom-code-reviewer`'s final report as
//! a review round (DESIGN D12).
//!
//! `subagent-stop.sh` pipes `{stage_id, session_id, agent_id, transcript_path}`
//! here once it has validated the stop event. For a v2 stage this reads the
//! reviewer's report from the transcript tail, parses its last `loom-review`
//! block, and writes `reviews/<stage>/round-<n>.json` at the worktree's
//! current change fingerprint; each suggestion becomes a `suggestion` entry in
//! the stage's memory journal. The report is read two ways, since a reviewer
//! may hand its report back through a tool call instead of ending its turn
//! with the report as plain text: the `message` input of the last
//! `SubagentHandback` tool call in an assistant entry, and the text of the
//! last assistant entry that has any. The hand-back is preferred; if both
//! parse and disagree, the hand-back wins and the round records why. A
//! missing or unreadable block still records the round, with `malformed` set
//! and no findings. The delegate always exits 0; a failure or a malformed
//! report is one line on stderr.
//!
//! Suggestions are journaled before the round is written, so if `write_round`
//! then fails (a racing duplicate harvest took the same round number, say),
//! those entries stay pending in the journal with no round naming them.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::commands::hook::review_transcript::reviewer_review;
use crate::fs::memory::{
    append_entry, validate_content, validate_evidence, MemoryEntry, MemoryEntryType,
};
use crate::fs::work_dir::WorkDir;
use crate::git::get_worktree_path;
use crate::models::stage::Stage;
use crate::validation::validate_id;
use crate::verify::review::fingerprint::{self, ChangeFingerprint};
use crate::verify::review::report::{single_line, ParsedReview, Suggestion};
use crate::verify::review::store::{self, RecordedFinding, ReviewRound, RECORD_VERSION};
use crate::verify::transitions::load_stage;

const MAX_INPUT_BYTES: u64 = 64 * 1024;
/// `validate_content`'s limit on a memory entry.
const MAX_ENTRY_BYTES: usize = 2000;

/// The stop event `subagent-stop.sh` pipes in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HarvestInput {
    pub stage_id: String,
    /// The parent Claude session; its `subagents/` directory holds the transcript.
    pub session_id: String,
    pub agent_id: String,
    pub transcript_path: PathBuf,
}

/// What one harvest did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Harvest {
    /// Not a v2 stage: nothing is written.
    Skipped,
    /// Round `round` is written; `malformed` says why its block was unreadable,
    /// `unjournaled` why some of its suggestions are not in the journal, and
    /// `discrepancy` why the hand-back and final text disagreed when both
    /// parsed.
    Recorded {
        round: u32,
        malformed: Option<String>,
        unjournaled: Option<String>,
        discrepancy: Option<String>,
    },
}

/// `loom hook review-harvest`: reads the stop event on stdin, `LOOM_WORK_DIR`
/// from the environment.
pub fn review_harvest() -> Result<()> {
    let outcome = read_input().and_then(|input| harvest(&work_dir_from_env()?, &input));
    match outcome {
        Ok(Harvest::Recorded {
            round,
            malformed,
            unjournaled,
            discrepancy,
        }) => {
            if let Some(reason) = malformed {
                eprintln!(
                    "loom hook review-harvest: review round {round} is malformed: {}",
                    single_line(&reason)
                );
            }
            if let Some(reason) = unjournaled {
                eprintln!(
                    "loom hook review-harvest: review round {round}: {}",
                    single_line(&reason)
                );
            }
            if let Some(reason) = discrepancy {
                eprintln!(
                    "loom hook review-harvest: review round {round}: {}",
                    single_line(&reason)
                );
            }
        }
        Ok(Harvest::Skipped) => {}
        Err(error) => eprintln!(
            "loom hook review-harvest: {}",
            single_line(&format!("{error:#}"))
        ),
    }
    Ok(())
}

/// Record the reviewer's report for `input.stage_id` in `work_dir`.
pub(crate) fn harvest(work_dir: &Path, input: &HarvestInput) -> Result<Harvest> {
    validate_id(&input.stage_id).context("invalid stage id")?;
    let stage = load_stage(&input.stage_id, work_dir)?;
    if stage.plan_version != 2 {
        return Ok(Harvest::Skipped);
    }
    let (parsed, discrepancy) = reviewer_review(input)?;
    let worktree = stage_worktree(work_dir, &stage)?;
    let target = crate::fs::resolve_target_branch_from_config(work_dir, &worktree)?;
    let current = fingerprint::compute(&worktree, &target)?;
    let number = store::next_round(work_dir, &stage.id)?;
    let (mut round, suggestions) = build_round(number, &input.agent_id, current, parsed)?;
    // Journal first: the round is immutable once written, so it may only name
    // the suggestion entries that exist.
    let (journaled, unjournaled) = journal_suggestions(work_dir, &stage.id, &suggestions);
    round.suggestion_memory_ids = journaled;
    store::write_round(work_dir, &stage.id, &round)?;
    Ok(Harvest::Recorded {
        round: number,
        malformed: round.malformed,
        unjournaled,
        discrepancy,
    })
}

/// Append `entries` to the stage's journal in order, stopping at the first
/// failure. Returns the ids appended and, on a failure, why the rest are not.
fn journal_suggestions(
    work_dir: &Path,
    stage_id: &str,
    entries: &[MemoryEntry],
) -> (Vec<String>, Option<String>) {
    let mut ids = Vec::new();
    for entry in entries {
        if let Err(error) = append_entry(work_dir, stage_id, entry) {
            let reason = format!(
                "journaled {} of {} suggestions: {error:#}",
                ids.len(),
                entries.len()
            );
            return (ids, Some(reason));
        }
        ids.push(entry.id.clone());
    }
    (ids, None)
}

fn read_input() -> Result<HarvestInput> {
    let mut input = String::new();
    io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_string(&mut input)
        .context("reading the stop event")?;
    ensure!(
        input.len() as u64 <= MAX_INPUT_BYTES,
        "the stop event is too large"
    );
    serde_json::from_str(&input).context("parsing the stop event")
}

fn work_dir_from_env() -> Result<PathBuf> {
    let work_dir = std::env::var_os("LOOM_WORK_DIR").context("LOOM_WORK_DIR is unset")?;
    let work_dir = PathBuf::from(work_dir);
    ensure!(work_dir.is_dir(), "LOOM_WORK_DIR is not a directory");
    Ok(work_dir)
}

/// The round for `parsed`, and the suggestion entries to journal for it.
fn build_round(
    number: u32,
    agent_id: &str,
    current: ChangeFingerprint,
    parsed: Result<ParsedReview, String>,
) -> Result<(ReviewRound, Vec<MemoryEntry>)> {
    let mut round = ReviewRound {
        version: RECORD_VERSION,
        round: number,
        agent_id: agent_id.to_string(),
        harvested_at: Utc::now(),
        fingerprint: current.value,
        files: current.files,
        malformed: None,
        findings: Vec::new(),
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    let review = match parsed {
        Ok(review) => review,
        Err(reason) => {
            round.malformed = Some(reason);
            return Ok((round, Vec::new()));
        }
    };
    round.findings = (1..)
        .zip(review.findings)
        .map(|(k, finding)| RecordedFinding {
            id: store::finding_id(number, k),
            finding,
        })
        .collect();
    let mut entries = Vec::new();
    for suggestion in &review.suggestions {
        // Never fails: content is non-empty and clipped to MAX_ENTRY_BYTES, evidence is a fixed
        // `review round <n>`. Keep both inside the memory validators or the round goes unwritten.
        entries.extend(suggestion_entry(suggestion, number)?);
    }
    round.resolved = review.resolved;
    round.unresolved = review.unresolved;
    Ok((round, entries))
}

/// A `suggestion` entry `<file>:<line> <text>`; none for a suggestion without text.
fn suggestion_entry(suggestion: &Suggestion, round: u32) -> Result<Option<MemoryEntry>> {
    let text = single_line(&suggestion.text);
    if text.is_empty() {
        return Ok(None);
    }
    let file = suggestion
        .file
        .as_deref()
        .map(single_line)
        .unwrap_or_default();
    let content = match (file.is_empty(), suggestion.line) {
        (true, _) => text,
        (false, Some(line)) => format!("{file}:{line} {text}"),
        (false, None) => format!("{file} {text}"),
    };
    let entry = MemoryEntry::new(MemoryEntryType::Suggestion, clip(content, MAX_ENTRY_BYTES))
        .with_evidence(vec![format!("review round {round}")]);
    validate_content(&entry.content)?;
    validate_evidence(&entry.evidence)?;
    Ok(Some(entry))
}

/// `text` cut to at most `max` bytes on a character boundary.
fn clip(mut text: String, max: usize) -> String {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

/// The stage's worktree in the project that holds `work_dir`.
fn stage_worktree(work_dir: &Path, stage: &Stage) -> Result<PathBuf> {
    let id = stage
        .worktree
        .as_deref()
        .with_context(|| format!("stage '{}' has no worktree", stage.id))?;
    validate_id(id).context("invalid worktree id")?;
    let state = WorkDir::new(work_dir)?;
    let project = state
        .project_root()
        .context("the state directory has no project root")?;
    let worktree = get_worktree_path(id, project);
    ensure!(
        worktree.is_dir(),
        "the worktree {} does not exist",
        worktree.display()
    );
    Ok(worktree)
}

#[cfg(test)]
#[path = "review_harvest_tests.rs"]
mod tests;
