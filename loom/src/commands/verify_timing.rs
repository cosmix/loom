//! When a standard stage's session may run `loom check`, and what the check
//! tells it to do next. A stage keeps one full check, run beside the final
//! review round; `loom stage complete` runs every criterion itself and reuses
//! passes on an unchanged tree. Two refusals enforce that before a new request
//! ticket is written, and `--force` overrides both:
//!
//! - a worker of the session has not returned (a check on a tree a worker is
//!   still editing is void when the worker returns);
//! - a full check once a review round is recorded.
//!
//! The worker evidence is `starts.jsonl` against `lifecycle.jsonl` of the
//! stage's subagent ledgers, plus the native Codex workers `native_gate`
//! reads. It does not classify transcripts as `loom subagents list` does, so
//! a worker whose stop event was dropped reads as outstanding: `--force` is
//! the way out.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

use super::CheckSelection;
use crate::commands::stage::native_gate::ensure_no_native_workers;
use crate::host_verify::{CriterionOutcome, JobState};
use crate::models::session::SessionType;
use crate::models::stage::{Stage, StageType};
use crate::provider_data::ledger::json_lines;
use crate::relay::emit::RelayContext;
use crate::subagent_lifecycle::{LifecycleRecord, LifecycleState, WorkerIdentity};
use crate::verify::review::gate::REVIEWER_AGENT_TYPE;
use crate::verify::review::store;

/// The exit code of a timing refusal; 0, 1 and 4 are the check's own.
pub(super) const REFUSED_EXIT: i32 = 3;

const FULL_PASSED: &str = "Next: these passes are stored for this tree. Do not run loom check \
     again. Fix review findings if any, then loom stage commit and loom stage complete, which \
     runs every criterion and reuses passes on an unchanged tree.";

const SELECTION_PASSED: &str = "Next: selected criteria pass. Do not run a full loom check; \
     continue to the re-review, then loom stage commit and loom stage complete.";

/// Whether the timing rules and next-step lines apply: the lead session of a
/// standard stage. Knowledge, integration-verify and the other stage types,
/// and merge-resolver and similar sessions, run `loom check` unrestricted.
fn applies(context: &RelayContext, stage: &Stage) -> bool {
    stage.stage_type == StageType::Standard
        && context
            .session_type
            .as_deref()
            .is_some_and(|session_type| session_type == SessionType::Stage.to_string())
}

/// The refusal text of a request for `selection`, or `None` when the check may
/// run: a session outside the timing rules, `force`, and a call that joins its
/// own pending request (`joins_pending`, asked last as it reads the tree) are
/// never refused, so a result can always be collected.
pub(super) fn refusal(
    context: &RelayContext,
    stage: &Stage,
    work_dir: &Path,
    (selection, force): (&CheckSelection, bool),
    joins_pending: impl FnOnce() -> bool,
) -> Option<String> {
    if force || !applies(context, stage) || joins_pending() {
        return None;
    }
    let (stage_id, session_id) = (stage.id.as_str(), context.session_id.as_str());
    // A run at the base commit never touches the worktree a worker edits.
    if !matches!(selection, CheckSelection::AtBase(_)) {
        if let Some(text) = worker_refusal(work_dir, stage_id, session_id) {
            return Some(text);
        }
    }
    if matches!(selection, CheckSelection::All) && round_recorded(work_dir, stage_id) {
        return Some(format!(
            "loom check refused: a review round is recorded for stage '{stage_id}', so the full \
             gate now belongs to `loom stage complete {stage_id}`. It runs every criterion on \
             the final tree and reuses passes on an unchanged tree. Re-check only the criteria \
             that failed: loom check {stage_id} --criterion <n>. Rounds left from an earlier \
             attempt of this stage are the case for --force."
        ));
    }
    None
}

fn round_recorded(work_dir: &Path, stage_id: &str) -> bool {
    store::next_round(work_dir, stage_id).is_ok_and(|next| next > 1)
}

fn worker_refusal(work_dir: &Path, stage_id: &str, session_id: &str) -> Option<String> {
    let workers = outstanding_workers(work_dir, stage_id, session_id);
    let native = ensure_no_native_workers(work_dir, stage_id).err();
    if workers.is_empty() && native.is_none() {
        return None;
    }
    let mut text = String::from("loom check refused: a worker of this session has not returned.");
    for (agent_type, agent_id) in &workers {
        let _ = write!(text, "\n  {agent_type} {agent_id}");
    }
    if let Some(error) = native {
        let _ = write!(text, "\n  {error}");
    }
    text.push_str(
        "\nA check on a tree a worker is still editing is void when the worker returns. Wait for \
         every worker (loom subagents watch), then run loom check again. If a named worker has \
         in fact returned, rerun with --force.",
    );
    Some(text)
}

/// `(agent type, agent id)` of each non-reviewer worker this session started
/// that has no recorded stop, in start order. A reviewer may be out: the one
/// full check runs beside the review.
fn outstanding_workers(work_dir: &Path, stage_id: &str, session_id: &str) -> Vec<(String, String)> {
    let dir = work_dir.join("subagents").join(stage_id);
    let returned = returned_agents(&dir.join("lifecycle.jsonl"), stage_id, session_id);
    let field = |row: &Value, name: &str| row.get(name).and_then(Value::as_str).map(str::to_owned);
    let mut seen = HashSet::new();
    json_lines(&dir.join("starts.jsonl"))
        .iter()
        .filter(|row| field(row, "stage_id").as_deref() == Some(stage_id))
        .filter(|row| field(row, "loom_session_id").as_deref() == Some(session_id))
        .filter_map(|row| Some((field(row, "agent_type")?, field(row, "agent_id")?)))
        .filter(|(agent_type, _)| agent_type != REVIEWER_AGENT_TYPE)
        .filter(|(_, agent_id)| !returned.contains(agent_id) && seen.insert(agent_id.clone()))
        .collect()
}

/// Ids of the Claude subagents of this session whose stop the lifecycle
/// journal records.
fn returned_agents(journal: &Path, stage_id: &str, session_id: &str) -> HashSet<String> {
    json_lines(journal)
        .into_iter()
        .filter_map(|row| serde_json::from_value::<LifecycleRecord>(row).ok())
        .filter(|record| {
            matches!(
                record.state,
                LifecycleState::Completed | LifecycleState::Failed | LifecycleState::Cancelled
            )
        })
        .filter_map(|record| match record.identity {
            WorkerIdentity::ClaudeSubagent {
                stage_id: stage,
                loom_session_id: session,
                agent_id,
                ..
            } if stage == stage_id && session == session_id => Some(agent_id),
            _ => None,
        })
        .collect()
}

/// The line that follows a finished check of `selection`, or `None` outside
/// the timing rules, for a run that is not a full or `--criterion` one, and
/// for a job that did not decide.
pub(super) fn next_step(
    (context, stage): (&RelayContext, &Stage),
    selection: &CheckSelection,
    state: JobState,
    outcomes: &[CriterionOutcome],
) -> Option<String> {
    if !applies(context, stage) {
        return None;
    }
    let stage_id = stage.id.as_str();
    let passed = match state {
        JobState::Passed => true,
        JobState::Failed => false,
        _ => return None,
    };
    match (selection, passed) {
        (CheckSelection::All, true) => Some(FULL_PASSED.to_string()),
        (CheckSelection::Criteria(_), true) => Some(SELECTION_PASSED.to_string()),
        (CheckSelection::All | CheckSelection::Criteria(_), false) => {
            Some(failed_step(stage_id, selection, outcomes))
        }
        _ => None,
    }
}

fn failed_step(
    stage_id: &str,
    selection: &CheckSelection,
    outcomes: &[CriterionOutcome],
) -> String {
    let failed = || outcomes.iter().filter(|outcome| !outcome.passed);
    let mut indices: Vec<usize> = failed()
        .filter_map(|outcome| outcome.label.strip_prefix("acceptance-")?.parse().ok())
        .collect();
    if let CheckSelection::Criteria(selected) = selection {
        // A job that failed with no outcome names no criterion: re-run them all.
        if outcomes.is_empty() {
            indices.clone_from(selected);
        }
    }
    let mut commands = Vec::new();
    if !indices.is_empty() {
        let flags: String = indices
            .iter()
            .map(|index| format!(" --criterion {index}"))
            .collect();
        commands.push(format!("loom check {stage_id}{flags}"));
    }
    if failed().any(|outcome| outcome.label.starts_with("contract")) {
        commands.push(format!("loom check {stage_id} --contracts"));
    }
    let head = "Next: give every failed criterion and every review finding to ONE fix worker.";
    if commands.is_empty() {
        return format!(
            "{head} Then continue to the re-review: loom stage complete re-runs the \
             goal-backward checks."
        );
    }
    format!("{head} After it returns, run: {}", commands.join("; "))
}

#[cfg(test)]
#[path = "verify_timing_tests.rs"]
mod tests;
