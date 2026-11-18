//! Read-only native worker summaries for list and harvest.

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::super::classify::{DoneEvidence, SubagentState, SubagentSummary};
use super::super::summary::empty;
use crate::native_worker::ids::REPORT_DEFAULT_BYTES;
use crate::native_worker::index::stage_units;
use crate::native_worker::model::{NativeUnitState, Outcome, Ownership, Phase};
use crate::native_worker::store::{invocation_dir, read_report};

pub(super) fn append_summaries(summaries: &mut Vec<SubagentSummary>, work_dir: Option<&Path>) {
    let (Some(work_dir), Ok(stage_id)) = (work_dir, std::env::var("LOOM_STAGE_ID")) else {
        return;
    };
    if crate::agent_state::is_safe_id(&stage_id) {
        summaries.extend(summaries_for_stage(work_dir, &stage_id, Utc::now()));
    }
}

fn summaries_for_stage(
    work_dir: &Path,
    stage_id: &str,
    now: DateTime<Utc>,
) -> Vec<SubagentSummary> {
    let mut seen = HashSet::new();
    let mut summaries = Vec::new();
    for record in stage_units(work_dir, stage_id) {
        match record {
            Ok(state) if seen.insert(state.manifest.unit_id.clone()) => {
                summaries.push(summary_for(work_dir, &state, now));
            }
            Ok(_) => {}
            Err(error) => eprintln!("native worker record unreadable: {error:?}"),
        }
    }
    summaries
}

fn summary_for(work_dir: &Path, worker: &NativeUnitState, now: DateTime<Utc>) -> SubagentSummary {
    let idle = (now - worker.last_progress_at).num_seconds().max(0);
    let mut summary = empty(
        format!("codex:{}", worker.manifest.unit_id),
        idle,
        Some("native-codex".into()),
    );
    summary.model = Some(format!("{} (requested)", worker.manifest.model));
    if worker.ownership == Ownership::Unverified {
        summary.state = SubagentState::ForwardUnknown;
        summary.terminal_reason =
            Some("native worker processes could not be proven stopped".into());
    } else if worker.phase != Phase::Terminal || worker.ownership == Ownership::Held {
        summary.state = SubagentState::ForwardWait;
    } else {
        apply_outcome(&mut summary, worker);
    }
    if matches!(
        summary.state,
        SubagentState::Done | SubagentState::Failed | SubagentState::Cancelled
    ) {
        summary.final_report = report_for(work_dir, worker);
    }
    summary
}

/// Maps a finished worker whose ownership is released onto its summary state.
fn apply_outcome(summary: &mut SubagentSummary, worker: &NativeUnitState) {
    match worker.outcome {
        Some(Outcome::Succeeded) => {
            summary.state = SubagentState::Done;
            summary.done_evidence = Some(DoneEvidence::Lifecycle);
        }
        Some(Outcome::Cancelled) => {
            summary.state = SubagentState::Cancelled;
            summary.terminal_reason.clone_from(&worker.outcome_reason);
        }
        Some(Outcome::Failed | Outcome::TimedOut) => {
            summary.state = SubagentState::Failed;
            let fallback = if worker.outcome == Some(Outcome::TimedOut) {
                "timed out"
            } else {
                "failed"
            };
            summary.terminal_reason = Some(
                worker
                    .outcome_reason
                    .clone()
                    .unwrap_or_else(|| fallback.into()),
            );
        }
        Some(Outcome::Unknown) | None => {
            summary.state = SubagentState::ForwardUnknown;
            summary.terminal_reason.clone_from(&worker.outcome_reason);
        }
    }
}

fn report_for(work_dir: &Path, worker: &NativeUnitState) -> Option<String> {
    let report = worker.report.as_ref()?;
    let dir = invocation_dir(
        work_dir,
        &worker.manifest.stage_id,
        &worker.manifest.invocation_id,
    );
    let (mut text, truncated) = read_report(&dir, REPORT_DEFAULT_BYTES).ok()??;
    if truncated {
        append_line(
            &mut text,
            &format!(
                "[report truncated at {REPORT_DEFAULT_BYTES} bytes; run 'loom subagents result --worker codex:{} --full']",
                worker.manifest.unit_id
            ),
        );
    }
    if !report.complete {
        append_line(
            &mut text,
            "[incomplete: the worker ended without a final answer]",
        );
    }
    Some(text)
}

fn append_line(text: &mut String, line: &str) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
}

#[cfg(test)]
#[path = "render_native_tests.rs"]
mod tests;
