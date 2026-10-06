//! The review gate at stage completion (DESIGN D12).
//!
//! A v2 `standard` or `integration-verify` stage completes only when a
//! well-formed review round is recorded, the latest one saw exactly the
//! worktree's current changes, and no finding, own or carried, is open.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use super::fingerprint::{self, ChangeFingerprint};
use super::report::single_line;
use super::store::{self, OpenFinding, ReviewRound};
use crate::models::stage::{Stage, StageType};

/// Whether the review gate and the test-integrity gate cover `stage`: a v2
/// `standard` or `integration-verify` stage.
pub fn covers(stage: &Stage) -> bool {
    stage.plan_version == 2
        && matches!(
            stage.stage_type,
            StageType::Standard | StageType::IntegrationVerify
        )
}

/// [`check`] against the worktree's fingerprint from the loom daemon that
/// owns it ([`fingerprint::compute`]), for a stage the gate [`covers`]; any
/// other stage passes. A stage the gate covers must have a worktree.
pub fn check_at_completion(
    stage: &Stage,
    work_dir: &Path,
    worktree_root: Option<&Path>,
    target_branch: &str,
) -> Result<()> {
    if !covers(stage) {
        return Ok(());
    }
    let Some(worktree_root) = worktree_root else {
        bail!(
            "Stage '{}': no worktree to check the review against",
            stage.id
        );
    };
    let current = fingerprint::compute(worktree_root, target_branch)
        .context("failed to compute the worktree's change fingerprint for the review gate")?;
    check(stage, work_dir, &current)
}

/// Fail, listing every problem at once, unless the latest well-formed review
/// round saw exactly the `current` changes and no finding is open. `current`
/// must come from the same observer as the rounds' fingerprints: the daemon.
pub fn check(stage: &Stage, work_dir: &Path, current: &ChangeFingerprint) -> Result<()> {
    let rounds = store::load_rounds(work_dir, &stage.id)?;
    let rulings = store::load_rulings(work_dir, &stage.id)?;
    let carried = store::load_carried(work_dir, &stage.id)?;
    let mut problems = Vec::new();
    match rounds.iter().rev().find(|round| round.is_well_formed()) {
        None => problems.push("no well-formed review round is recorded".to_string()),
        Some(latest) => problems.extend(stale_review(latest, current)),
    }
    let open = store::open_among(&rounds, &rulings, &carried);
    problems.extend(open.iter().map(describe_open));
    if problems.is_empty() {
        return Ok(());
    }
    let mut message = failure_message(&stage.id, &problems, !open.is_empty(), rounds.last());
    if let Some(hint) = harvest_hint(work_dir, &stage.id, rounds.len()) {
        message.push('\n');
        message.push_str(&hint);
    }
    bail!("{message}")
}

/// The agent type whose spawns and skipped stops the hint counts.
const REVIEWER_AGENT_TYPE: &str = "loom-code-reviewer";
/// Ledger files a session can write are read up to this many bytes.
const MAX_LEDGER_BYTES: u64 = 1024 * 1024;
/// Skipped-stop lines the hint prints (the latest ones).
const MAX_SKIP_LINES: usize = 5;
/// Characters of a ledger `agent_id` or `reason` the hint prints.
const MAX_FIELD_CHARS: usize = 64;

/// Why a reviewer stop may not have produced a round: when the stage's hook
/// ledgers show more `loom-code-reviewer` spawns than recorded `rounds`, the
/// count of unharvested stops and the reasons the SubagentStop hook logged
/// for skipping them. `None` when every spawn has a round or the ledgers are
/// unreadable; advisory text, so it never fails.
pub fn harvest_hint(work_dir: &Path, stage_id: &str, rounds: usize) -> Option<String> {
    let dir = work_dir.join("subagents").join(stage_id);
    let spawns: BTreeSet<String> = reviewer_rows(&dir.join("starts.jsonl"))
        .iter()
        .filter_map(|row| Some(row.get("agent_id")?.as_str()?.to_owned()))
        .collect();
    let unharvested = spawns.len().checked_sub(rounds).filter(|k| *k > 0)?;
    let skips = reviewer_rows(&dir.join("stop-skips.jsonl"));
    let latest = skips.len().saturating_sub(MAX_SKIP_LINES);
    let mut hint = format!(
        "Reviewer stop events: {} reviewer spawns, {rounds} rounds, \
         {unharvested} stop events not harvested.",
        spawns.len()
    );
    for row in &skips[latest..] {
        let field = |name| ledger_text(row.get(name).and_then(Value::as_str).unwrap_or(""));
        hint.push_str(&format!(
            "\n  skipped stop: agent {}: {}",
            field("agent_id"),
            field("reason")
        ));
    }
    hint.push_str(
        "\nRun the review again with LOOM_HOOK_DEBUG=1 to see why the SubagentStop hook \
         skipped a stop.",
    );
    Some(hint)
}

/// The parsable rows of a JSONL ledger whose `agent_type` is the reviewer.
/// A symlink, a non-file, an oversized file and an unparsable line yield
/// nothing: a session can write these files.
fn reviewer_rows(path: &Path) -> Vec<Value> {
    let Some(text) = read_ledger(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row.get("agent_type").and_then(Value::as_str) == Some(REVIEWER_AGENT_TYPE))
        .collect()
}

fn read_ledger(path: &Path) -> Option<String> {
    let file: File = OpenOptions::new()
        .read(true)
        // O_NONBLOCK keeps a planted FIFO from blocking the open; `is_file`
        // below then refuses it.
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_LEDGER_BYTES {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_LEDGER_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

/// `text` reduced to `[A-Za-z0-9_.-]`, at most [`MAX_FIELD_CHARS`] characters.
fn ledger_text(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(MAX_FIELD_CHARS)
        .collect()
}

/// Why `latest` no longer covers the `current` changes, if it does not.
fn stale_review(latest: &ReviewRound, current: &ChangeFingerprint) -> Option<String> {
    if current.value == latest.fingerprint {
        return None;
    }
    let changed = fingerprint::changed_since(&latest.files, &current.files);
    let since = if changed.is_empty() {
        "the base commit changed".to_string()
    } else {
        let paths: Vec<String> = changed.iter().map(|path| single_line(path)).collect();
        format!("changed since: {}", paths.join(", "))
    };
    Some(format!(
        "review round {} saw {}, but the worktree is now at {} ({since})",
        latest.round, latest.fingerprint, current.value
    ))
}

fn describe_open(open: &OpenFinding) -> String {
    let finding = &open.finding;
    format!(
        "open finding {} ({}) {}:{}: {}",
        open.id,
        single_line(&finding.severity),
        single_line(&finding.file),
        finding.line,
        single_line(&finding.claim)
    )
}

fn failure_message(
    stage_id: &str,
    problems: &[String],
    has_open: bool,
    last_round: Option<&ReviewRound>,
) -> String {
    let mut message = format!(
        "review gate failed for stage '{stage_id}':\n  - {}",
        problems.join("\n  - ")
    );
    let malformed = last_round.and_then(|round| Some((round.round, round.malformed.as_ref()?)));
    if let Some((round, reason)) = malformed {
        message.push_str(&format!(
            "\nThe latest review round ({round}) is malformed: {:?}",
            single_line(reason)
        ));
    }
    if has_open {
        message.push_str(&format!(
            "\nOpen findings: fix them and run a re-review, or dispute them together with \
             `loom stage dispute-findings {stage_id} --finding <id> ... --reason ...`."
        ));
    }
    message.push_str(&format!(
        "\nRun `loom stage review status {stage_id}` for the rounds, the open findings and \
         the files changed since the latest review."
    ));
    message
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "gate_hint_tests.rs"]
mod hint_tests;
