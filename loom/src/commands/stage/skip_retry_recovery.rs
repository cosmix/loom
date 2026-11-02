//! The requeue of a retry that briefs its next session with a recovery signal.

use anyhow::{Context, Result};
use std::path::Path;

use super::super::recover::{
    extract_context_tokens, find_crash_report, generate_recovery_session_id, load_last_heartbeat,
    recovery_close_reason, with_operator_answer,
};
use super::{persist_retry_delta, report};
use crate::fs::work_dir::resolve_context_ceiling_tokens;
use crate::git::worktree::find_repo_root_from_cwd;
use crate::hooks::read_stage_events;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::merge_lifecycle::MergeLifecycle;
use crate::orchestrator::signals::{
    generate_recovery_signal, LastHeartbeatInfo, RecoveryReason, RecoverySignalContent,
};

/// Why a retry writes a recovery signal, and what it captured from the stage
/// before planning the requeue.
pub(super) struct Recovery<'a> {
    pub(super) reason: RecoveryReason,
    pub(super) previous_session_id: &'a str,
    pub(super) attempt: u32,
    pub(super) context: Option<&'a str>,
}

/// Write the recovery signal, then persist the requeue that points at it.
pub(super) fn requeue_with_recovery(
    work_dir: &Path,
    stage_id: &str,
    stage: &mut Stage,
    original: (&StageStatus, Option<&str>),
    force: bool,
    recovery: &Recovery<'_>,
) -> Result<()> {
    let last_heartbeat = load_last_heartbeat(work_dir, stage_id);

    let recent_events = read_stage_events(work_dir, stage_id).unwrap_or_default();
    let last_event = recent_events
        .last()
        .map(|e| format!("{}: {}", e.event, e.timestamp));

    let new_session_id = generate_recovery_session_id(stage_id);
    let signal_content = signal_content(
        work_dir,
        stage_id,
        stage,
        &new_session_id,
        recovery,
        last_heartbeat,
    );

    reconcile_overlay_before_signal(work_dir, stage_id)?;

    let signal_content = with_operator_answer(signal_content, recovery.context);
    generate_recovery_signal(&signal_content, stage, work_dir)
        .context("Failed to generate recovery signal")?;

    stage.session = Some(new_session_id.clone());
    stage.close_reason = Some(recovery_close_reason(recovery.attempt, recovery.context));
    stage.updated_at = chrono::Utc::now();

    persist_retry_delta(stage_id, work_dir, stage, original, force)?;

    report::recovery_started(
        stage_id,
        recovery.previous_session_id,
        &new_session_id,
        &recovery.reason,
        recovery.attempt,
        recovery.context,
        last_event.as_deref(),
    );
    Ok(())
}

/// The retrieval pack is embedded into the signal content at signal-WRITE time
/// here, not at the next spawn - unlike a fresh spawn, `start_stage` reuses these
/// bytes verbatim from disk and never regenerates them. So the overlay must be
/// current NOW, or this retry (and crash/hang auto-recovery, which share this
/// path) briefs the agent from the pre-crash tree. `reconcile_overlay` already
/// early-returns when the worktree is missing (merge_lifecycle.rs:76-79), which
/// is correct here: a stage whose worktree was cleaned up has nothing to reconcile.
fn reconcile_overlay_before_signal(work_dir: &Path, stage_id: &str) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or_else(|| cwd.clone());
    MergeLifecycle::new(stage_id, &repo_root, work_dir).reconcile_overlay();
    Ok(())
}

/// The signal content `recovery.reason` calls for.
fn signal_content(
    work_dir: &Path,
    stage_id: &str,
    stage: &Stage,
    new_session_id: &str,
    recovery: &Recovery<'_>,
    last_heartbeat: Option<LastHeartbeatInfo>,
) -> RecoverySignalContent {
    let session_id = new_session_id.to_string();
    let stage_id = stage_id.to_string();
    let previous = recovery.previous_session_id;
    match recovery.reason {
        RecoveryReason::Crash => RecoverySignalContent::for_crash(
            session_id,
            stage_id,
            previous.to_string(),
            find_crash_report(work_dir, previous),
            recovery.attempt,
        ),
        RecoveryReason::Hung => RecoverySignalContent::for_hung(
            session_id,
            stage_id,
            previous.to_string(),
            last_heartbeat,
            recovery.attempt,
        ),
        RecoveryReason::ContextExhaustion => {
            let context_tokens = stage
                .close_reason
                .as_ref()
                .and_then(|r| extract_context_tokens(r))
                .unwrap_or_else(|| {
                    resolve_context_ceiling_tokens(work_dir, stage.context_ceiling_tokens)
                });
            RecoverySignalContent::for_context_exhaustion(
                session_id,
                stage_id,
                previous.to_string(),
                context_tokens,
                recovery.attempt,
            )
        }
        RecoveryReason::Manual => RecoverySignalContent::for_manual(
            session_id,
            stage_id,
            previous.to_string(),
            recovery.attempt,
        ),
    }
}
