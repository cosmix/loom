//! Diagnosis for an empty live-session set.
//!
//! A flat "No live tmux sessions" cannot tell an operator whether a session
//! record exists and failed one of `viewer::load_live_tmux_session`'s
//! filters, or whether a stage claims to be executing with no session record
//! at all — the two read identically and the second is the more dangerous
//! misreading (an agent can still be running with nothing left to find it
//! by). This module walks every session record on disk, names the filter
//! that rejected each one, and separately calls out any `Executing` stage no
//! record names.

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::models::session::{Session, SessionBackendKind, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::tmux::viewer::tmux_session_name;
use crate::orchestrator::terminal::tmux::TmuxBackend;
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::list_all_stages;

/// Every session record in `<work_dir>/sessions`, regardless of whether it is
/// currently attachable — the raw input to the per-record diagnosis below.
/// Skips unreadable or corrupt files exactly like `viewer::live_tmux_sessions`
/// does: a bad file is not something this diagnosis can explain either, and
/// one bad file must not hide the diagnosis for every other record.
fn all_session_records(work_dir: &Path) -> Vec<Session> {
    let Ok(entries) = fs::read_dir(work_dir.join("sessions")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
        .filter_map(|entry| {
            let content = fs::read_to_string(entry.path()).ok()?;
            parse_from_markdown::<Session>(&content, "Session").ok()
        })
        .collect()
}

/// Which of `viewer::load_live_tmux_session`'s checks rejected `session`, in
/// the same order that function applies them, so this diagnosis can never
/// disagree with actual discovery. `is_alive` is injected exactly like
/// `viewer::attachable_panes`'s `ready` parameter: production always passes
/// [`TmuxBackend::is_session_alive`]; tests supply a canned answer without
/// touching the filesystem.
///
/// The final branch is defensive, not reachable in production: a session
/// whose PID evidence resolves to alive always had a resolvable tracking
/// name to resolve it from (see `viewer::tmux_session_name`'s doc comment).
/// It exists so this function still explains itself if that invariant is
/// ever broken, rather than silently mis-reporting the session as live.
fn attach_rejection_reason(
    session: &Session,
    is_alive: impl FnOnce(&Session) -> Result<bool>,
) -> String {
    if session.backend != SessionBackendKind::Tmux {
        return format!("backend is {}", session.backend);
    }
    if !matches!(
        session.status,
        SessionStatus::Running | SessionStatus::Spawning
    ) {
        return format!("status is {}", session.status);
    }
    if !is_alive(session).unwrap_or(false) {
        return "process is not running".to_string();
    }
    if tmux_session_name(session).is_none() {
        return "no tracking key".to_string();
    }
    "no longer matches the live filters".to_string()
}

/// One line naming a session record and why it is not attachable.
fn record_rejection_line(session: &Session, reason: &str) -> String {
    let stage = session.stage_id.as_deref().unwrap_or("(no stage)");
    format!("  session {} (stage {stage}): {reason}", session.id)
}

/// Stage ids with status `Executing` that no session record — live or not —
/// names: the orphan case, where the daemon lost track of (or never wrote) a
/// session for a stage while its agent may still be running. Pure function of
/// its inputs so it is testable with fixture data; the caller does the I/O.
fn orphaned_stage_ids(stages: &[Stage], sessions: &[Session]) -> Vec<String> {
    stages
        .iter()
        .filter(|stage| stage.status == StageStatus::Executing)
        .filter(|stage| {
            !sessions
                .iter()
                .any(|s| s.stage_id.as_deref() == Some(stage.id.as_str()))
        })
        .map(|stage| stage.id.clone())
        .collect()
}

/// One line explaining an orphaned stage and both ways out: adoption if the
/// agent is still alive, or a manual reset if it is not. Does not suggest
/// `tmux attach` — loom's tmux sockets live in a private directory a bare
/// `tmux attach` can never see (see the parent module's docs).
fn orphaned_stage_line(stage_id: &str) -> String {
    format!(
        "  Stage '{stage_id}' claims Executing with no session record naming it - if the agent \
         is still alive the daemon will adopt it on its next poll; otherwise `loom stage reset \
         --kill-session {stage_id}` clears it."
    )
}

/// Message for the case where `<work_dir>/sessions` holds no records at all.
fn no_session_records_message(work_dir: &Path) -> String {
    format!(
        "No session records in {}",
        work_dir.join("sessions").display()
    )
}

/// Diagnose an empty live set: walk every session record (not just the live
/// ones) and report which filter rejected each, then separately call out any
/// stage that claims `Executing` with no session record naming it at all.
pub(super) fn diagnose_empty_live_set(work_dir: &Path) {
    let records = all_session_records(work_dir);

    if records.is_empty() {
        println!("{}", no_session_records_message(work_dir));
    } else {
        let backend = TmuxBackend::new(work_dir.to_path_buf());
        for session in &records {
            let reason = attach_rejection_reason(session, |s| backend.is_session_alive(s));
            println!("{}", record_rejection_line(session, &reason));
        }
    }

    let stages = list_all_stages(work_dir).unwrap_or_default();
    for stage_id in orphaned_stage_ids(&stages, &records) {
        println!("{}", orphaned_stage_line(&stage_id));
    }
}

#[cfg(test)]
#[path = "tests/diagnose.rs"]
mod tests;
