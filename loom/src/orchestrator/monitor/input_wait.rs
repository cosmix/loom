//! Reconciliation for a stage stuck in `WaitingForInput` whose session never
//! actually paused for a human.
//!
//! `loom stage waiting` is driven by the `PreToolUse:AskUserQuestion` hook,
//! but Claude Code has internal paths (a `confirmWithUser` helper, a plugin
//! `ui.ask` bridge) that can trigger the same permission pipeline without a
//! `tool_use` ever landing in the transcript. When that happens the hook
//! fires on a phantom question, the stage flips to `WaitingForInput`, and the
//! session keeps executing tools for hours with nothing actually waiting on
//! a person. Nothing else reconciles that state, so the stage is stuck until
//! a human notices `loom stage complete` refusing to run.
//!
//! This module detects exactly that condition — the stage's own session made
//! useful progress *after* the transition into `WaitingForInput` — and moves
//! it back to `Executing`. The same progress is how a contract writer parked
//! on a refused freeze resumes once the operator types into its session
//! (`verify::contracts::refusal`). A stage that flips to `WaitingForInput` and then
//! goes quiet (a real question) is left alone. Only progress from the MAIN
//! agent counts: a background subagent keeps calling tools no matter what the
//! main agent is doing, so its heartbeat says nothing about whether the main
//! agent is still blocked on a person. Only a main-agent heartbeat that names
//! the tool it ran counts as that progress; a lifecycle heartbeat (subagent
//! stop, teammate idle, session start) carries no tool and proves nothing
//! about the main agent either.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::models::stage::{Stage, StageStatus};
use crate::verify::contracts::refusal::end_wait;

use super::heartbeat::{Heartbeat, HeartbeatWatcher};

/// If `stage`'s `WaitingForInput` wait looks stale, return the heartbeat that
/// proves it and the time it last made progress.
///
/// A wait is stale when the stage's own session kept making progress after
/// the transition into `WaitingForInput`, which means nothing was actually
/// waiting on a person. Returns `None` for every other case: no wait, no
/// session, no heartbeat (or one from a different session), a heartbeat
/// written by a subagent rather than the main agent, a lifecycle heartbeat
/// that names no tool, an in-flight question the post hook still owns, or a
/// heartbeat no newer than the wait itself.
fn stale_wait_progress<'a>(
    stage: &Stage,
    heartbeats: &'a HeartbeatWatcher,
) -> Option<(&'a Heartbeat, DateTime<Utc>)> {
    if stage.status != StageStatus::WaitingForInput {
        return None;
    }
    let session_id = stage.session.as_deref()?;
    let heartbeat = heartbeats.get_heartbeat(&stage.id)?;
    // A heartbeat left behind by a previous session for this stage never
    // counts: it says nothing about whether the CURRENT wait is real.
    if heartbeat.session_id != session_id {
        return None;
    }
    // A subagent's tool call says nothing about whether the MAIN agent is
    // blocked on a question; only the main agent's own progress proves the
    // wait is stale.
    if heartbeat.subagent {
        return None;
    }
    // A heartbeat with no named tool is a lifecycle record (subagent stop,
    // teammate idle, session start), not the main agent executing a tool, so
    // it proves nothing about whether the main agent is still blocked. An
    // AskUserQuestion heartbeat IS the answered question; the post hook owns it.
    match heartbeat.last_tool.as_deref() {
        None | Some("AskUserQuestion") => return None,
        Some(_) => {}
    }
    let progress_at = heartbeat.effective_progress_at();
    if progress_at <= stage.updated_at {
        return None;
    }

    Some((heartbeat, progress_at))
}

/// Move `stage` back to `Executing` because `heartbeat` shows it never
/// actually stopped for a person. Returns whether the stage was resumed.
fn resume_stale_wait(
    work_dir: &Path,
    stage: &Stage,
    session_id: &str,
    heartbeat: &Heartbeat,
    progress_at: DateTime<Utc>,
) -> bool {
    match crate::verify::transitions::update_stage(&stage.id, work_dir, |stage| {
        if stage.status == StageStatus::WaitingForInput {
            end_wait(stage)
        } else {
            Ok(())
        }
    }) {
        Ok(_) => {
            tracing::warn!(
                stage_id = %stage.id,
                session_id = %session_id,
                last_tool = ?heartbeat.last_tool,
                progress_at = %progress_at,
                waiting_since = %stage.updated_at,
                "Stage left waiting-for-input: its session kept executing tools after \
                 the transition, so nothing was waiting on the user"
            );
            true
        }
        Err(error) => {
            tracing::warn!(
                stage_id = %stage.id,
                session_id = %session_id,
                error = %error,
                "Failed to auto-resume a stale waiting-for-input stage"
            );
            false
        }
    }
}

/// Return the ids of the stages that were moved back to `Executing`.
pub(super) fn reconcile_stale_input_waits(
    work_dir: &Path,
    stages: &[Stage],
    heartbeats: &HeartbeatWatcher,
) -> Vec<String> {
    let mut resumed = Vec::new();

    for stage in stages {
        let Some((heartbeat, progress_at)) = stale_wait_progress(stage, heartbeats) else {
            continue;
        };
        // stale_wait_progress only returns Some when the stage has a session.
        let session_id = stage
            .session
            .as_deref()
            .expect("checked by stale_wait_progress");

        if resume_stale_wait(work_dir, stage, session_id, heartbeat, progress_at) {
            resumed.push(stage.id.clone());
        }
    }

    resumed
}
