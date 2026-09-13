//! Per-session housekeeping each tick (section 8, drain steps 8-9): retire the
//! relay state of sessions that are gone, and warn about tickets the relay
//! never picked up.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::fs::inbox::session_inbox;
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::{cleanup_session_settings, session_settings_path};
use crate::relay::{session_dir, RequestKind, Ticket, MAX_TICKET_BYTES};

use super::entry::{self, Regular};
use super::session_pass::{drain_logged, load_record};
use super::{InboxHost, PassReport, Tick};

/// A non-telemetry ticket older than this means the relay is not working.
const STALE_TICKET_SECS: i64 = 60;
/// Telemetry tickets are best-effort; older ones are deleted silently.
const TELEMETRY_TICKET_TTL_SECS: i64 = 10 * 60;
/// The one inbox file retirement keeps: `loom request status` answers from it.
const LEDGER_FILE: &str = "ledger.jsonl";

/// Visit every session this state directory records.
pub(super) fn sweep_sessions(host: &mut dyn InboxHost, tick: &Tick<'_>, report: &mut PassReport) {
    let Ok(read_dir) = std::fs::read_dir(host.work_dir().join("sessions")) else {
        return;
    };
    let mut ids: Vec<String> = read_dir
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = name.strip_suffix(".md")?.to_string();
            crate::validation::validate_id(&id).ok()?;
            Some(id)
        })
        .collect();
    ids.sort();
    for sid in ids {
        sweep_one(host, tick, &sid, report);
    }
}

fn sweep_one(host: &mut dyn InboxHost, tick: &Tick<'_>, sid: &str, report: &mut PassReport) {
    let scratch = tick
        .scratch_root
        .and_then(|root| session_dir(root, sid).ok())
        .filter(|dir| dir.symlink_metadata().is_ok());
    // Cheap before the record is parsed: a session with nothing left to
    // retire — no scratch directory, no undrained inbox entries, and no
    // settings capsule of its own — costs one stat per tick. A capsule can
    // outlive its scratch directory (e.g. an operator wiping scratch state
    // separately), so it is checked for on its own rather than assumed to
    // track the scratch directory's lifetime.
    if scratch.is_none()
        && !has_inbox_leftovers(host.work_dir(), sid)
        && !has_capsule(host.work_dir(), sid)
    {
        return;
    }
    let Ok(Some(record)) = load_record(host.work_dir(), sid) else {
        return;
    };
    if record.status == SessionStatus::Running {
        if let Some(dir) = scratch {
            check_tickets(host, tick, sid, &dir, report);
        }
        return;
    }
    if matches!(host.session_alive(&record), Ok(false)) {
        retire(host, sid, &record, scratch.as_deref(), report);
    }
}

/// Inbox content other than the ledger retirement keeps.
fn has_inbox_leftovers(work_dir: &Path, sid: &str) -> bool {
    session_inbox(work_dir, sid)
        .ok()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .is_some_and(|mut names| {
            names.any(|name| name.is_ok_and(|name| name.file_name() != LEDGER_FILE))
        })
}

/// True when the session still has a generated settings capsule on disk.
fn has_capsule(work_dir: &Path, sid: &str) -> bool {
    session_settings_path(work_dir, sid)
        .symlink_metadata()
        .is_ok()
}

/// Retire a session whose record is no longer Running and whose process is
/// confirmed gone: drain its inbox a last time, fold its permission approvals
/// back, then delete its scratch directory, any undrained entries and `.tmp/`,
/// and its capsule. The ledger stays.
fn retire(
    host: &mut dyn InboxHost,
    sid: &str,
    record: &Session,
    scratch: Option<&Path>,
    report: &mut PassReport,
) {
    if !drain_logged(host, sid, record, report) {
        return; // its entries wait for the next tick, and so does the cleanup
    }
    let work_dir = host.work_dir().to_path_buf();
    fold_back_permissions(record, host.repo_root());
    let mut failures = Vec::new();
    if let Some(dir) = scratch {
        if let Err(error) = entry::remove_tree(dir) {
            failures.push(format!("{error:#}"));
        }
    }
    if let Err(error) = remove_inbox_leftovers(&work_dir, sid) {
        failures.push(format!("{error:#}"));
    }
    cleanup_session_settings(&work_dir, sid);
    if failures.is_empty() {
        tracing::info!(session_id = %sid, "Retired the relay state of a finished session");
        report.retired.push(sid.to_string());
    } else if host.first_report(&format!("retire:{sid}")) {
        tracing::warn!(session_id = %sid, failures = %failures.join("; "), "Could not fully retire a finished session's relay state; retrying next tick");
    }
}

/// The permission fold-back stage completion runs, which a broker completion
/// never reaches: it records the session's approvals into the loom-owned
/// list. Best-effort; retirement never fails over it.
fn fold_back_permissions(record: &Session, repo_root: &Path) {
    let checkout = record
        .worktree_path
        .clone()
        .unwrap_or_else(|| repo_root.to_path_buf());
    if let Err(error) = crate::fs::permissions::sync_worktree_permissions(&checkout, repo_root) {
        tracing::warn!(session_id = %record.id, error = %format!("{error:#}"), "Permission fold-back at session retirement failed");
    }
}

/// Everything in `W/inbox/<sid>/` but the ledger. A symlink standing in for
/// the directory is removed by name, never followed.
fn remove_inbox_leftovers(work_dir: &Path, sid: &str) -> Result<()> {
    let dir = session_inbox(work_dir, sid)?;
    match std::fs::symlink_metadata(&dir) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return entry::remove_name(&dir),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to stat {}", dir.display()))
        }
    }
    let names =
        std::fs::read_dir(&dir).with_context(|| format!("failed to list {}", dir.display()))?;
    for name in names {
        let name = name.with_context(|| format!("failed to list {}", dir.display()))?;
        if name.file_name() != LEDGER_FILE {
            entry::remove_tree(&name.path())?;
        }
    }
    Ok(())
}

/// A Running session's scratch tickets: a non-telemetry one past
/// [`STALE_TICKET_SECS`] means its relay is not working (warned once per
/// session); a telemetry one past [`TELEMETRY_TICKET_TTL_SECS`] is deleted.
fn check_tickets(
    host: &mut dyn InboxHost,
    tick: &Tick<'_>,
    sid: &str,
    dir: &Path,
    report: &mut PassReport,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let mut stalled = 0usize;
    for name in read_dir.flatten() {
        let path = name.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("req") {
            continue;
        }
        let Some(age) = age_secs(&path, tick.now) else {
            continue;
        };
        if ticket_kind(&path) == Some(RequestKind::Telemetry) {
            if age > TELEMETRY_TICKET_TTL_SECS {
                let _ = entry::remove_name(&path);
            }
        } else if age > STALE_TICKET_SECS {
            stalled += 1;
        }
    }
    if stalled > 0 && host.first_report(&format!("stalled:{sid}")) {
        tracing::warn!(session_id = %sid, stalled, "relay stalled: request tickets older than 60 s were never relayed; the loom relay hook is not picking them up");
        report.stalled_warned.push(sid.to_string());
    }
}

fn age_secs(path: &Path, now: DateTime<Utc>) -> Option<i64> {
    let modified = path.symlink_metadata().ok()?.modified().ok()?;
    Some((now - DateTime::<Utc>::from(modified)).num_seconds())
}

/// The kind a ticket names; `None` for anything that does not read as one.
fn ticket_kind(path: &Path) -> Option<RequestKind> {
    match entry::read_regular(path, MAX_TICKET_BYTES) {
        Ok(Regular::Bytes(bytes)) => Ticket::decode(&bytes).ok().map(|ticket| ticket.kind),
        _ => None,
    }
}
