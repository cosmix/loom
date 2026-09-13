//! Draining one session's inbox: the at-most-once ledger protocol around the
//! handlers in `apply` (section 8, drain steps 1-7).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};

use crate::fs::inbox::{
    append_ledger, inbox_root, pending_entries, read_ledger, session_inbox, validate_request_id,
    LedgerOutcome, LedgerRecord, LedgerState,
};
use crate::fs::safe_read::{is_not_found, read_to_string_bounded};
use crate::models::session::Session;
use crate::parser::frontmatter::parse_from_markdown;
use crate::relay::{InboxEntry, RequestKind};

use super::entry::{self, ReadEntry};
use super::{apply, InboxHost, PassReport, Settle, Tick};

/// Upper bound on a session record, matching the daemon's other readers.
const MAX_SESSION_FILE_BYTES: usize = 1024 * 1024;
/// An inbox directory whose session has no record is removed after this long.
const ORPHAN_INBOX_SECS: i64 = 24 * 60 * 60;

/// Drain every session directory under `W/inbox/`.
pub(super) fn drain_inboxes(host: &mut dyn InboxHost, tick: &Tick<'_>, report: &mut PassReport) {
    let root = inbox_root(host.work_dir());
    let Ok(read_dir) = std::fs::read_dir(&root) else {
        return; // no inbox yet: nothing has been relayed
    };
    let mut names: Vec<String> = read_dir
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        if let Err(reason) = entry::check_session_dir(&root, &name) {
            if host.first_report(&format!("dir:{name}")) {
                tracing::warn!(dir = %name, %reason, "Skipping an inbox directory that is not a plain session directory");
            }
            continue;
        }
        match load_record(host.work_dir(), &name) {
            Ok(Some(record)) => {
                drain_logged(host, &name, &record, report);
            }
            Ok(None) => handle_orphan(host, tick, &root.join(&name), &name),
            Err(error) => {
                if host.first_report(&format!("record:{name}")) {
                    tracing::warn!(session_id = %name, error = %format!("{error:#}"), "Cannot read the session record behind an inbox; its entries wait");
                }
            }
        }
    }
}

/// [`drain_session`], reporting an I/O failure once until the session drains
/// cleanly again. Returns whether the drain completed.
pub(super) fn drain_logged(
    host: &mut dyn InboxHost,
    sid: &str,
    record: &Session,
    report: &mut PassReport,
) -> bool {
    let key = format!("io:{sid}");
    match drain_session(host, sid, record, report) {
        Ok(()) => {
            host.clear_report(&key);
            true
        }
        Err(error) => {
            if host.first_report(&key) {
                tracing::warn!(session_id = %sid, error = %format!("{error:#}"), "Inbox drain failed; entries stay pending and retry next tick");
            }
            false
        }
    }
}

/// Apply every pending entry in `sid`'s inbox. `Err` only on I/O failure,
/// which leaves the entry being handled, and every later one, in place.
pub(super) fn drain_session(
    host: &mut dyn InboxHost,
    sid: &str,
    record: &Session,
    report: &mut PassReport,
) -> Result<()> {
    let work_dir = host.work_dir().to_path_buf();
    let ledger = load_ledger(&work_dir, sid)?;
    let mut pass = SessionPass {
        host,
        work_dir,
        sid,
        record,
        ledger,
        report,
    };
    pass.run()
}

/// One session's drain, with the ledger ids it already holds.
struct SessionPass<'a> {
    host: &'a mut dyn InboxHost,
    work_dir: PathBuf,
    sid: &'a str,
    record: &'a Session,
    ledger: HashSet<String>,
    report: &'a mut PassReport,
}

impl SessionPass<'_> {
    fn run(&mut self) -> Result<()> {
        let pending = pending_entries(&self.work_dir, self.sid)?;
        let dir = session_inbox(&self.work_dir, self.sid)?;
        for name in &pending.skipped {
            let reason = "the entry is not a regular single-link file within the size cap";
            self.discard(&dir.join(name), name, reason)?;
        }
        let mut entries = Vec::new();
        for path in pending.entries {
            let Some(id) = file_id(&path) else {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                self.discard(&path, &name, "the file name is not a request id")?;
                continue;
            };
            if self.ledger.contains(&id) {
                entry::remove_name(&path)?; // already settled: a duplicate relay
                continue;
            }
            match entry::read_entry(&path)? {
                ReadEntry::Entry(inbox_entry) => entries.push((path, id, inbox_entry)),
                ReadEntry::Malformed { kind, reason } => {
                    self.refuse_malformed(&path, &id, kind, reason)?;
                }
            }
        }
        entries.sort_by(|a, b| (a.2.relayed_at, &a.1).cmp(&(b.2.relayed_at, &b.1)));
        for (path, id, inbox_entry) in entries {
            self.settle(&path, &id, inbox_entry)?;
        }
        Ok(())
    }

    /// Validate, apply through the matrix and handler, record the outcome,
    /// then delete the entry — `applying` first, so a crash mid-handler is
    /// never retried.
    fn settle(&mut self, path: &Path, id: &str, inbox_entry: InboxEntry) -> Result<()> {
        let kind = inbox_entry.kind;
        let admitted = validate_attribution(self.sid, self.record, id, &inbox_entry)
            .and_then(|()| apply::admit(self.record, &inbox_entry));
        let settle = match admitted {
            Err(reason) => Settle::Refused(reason),
            Ok(admitted) => {
                self.append(id, kind, Some(LedgerState::Applying), None, None)?;
                apply::apply(&mut *self.host, self.record, admitted)
            }
        };
        let (outcome, reason) = match &settle {
            Settle::Applied(note) => (LedgerOutcome::Applied, note.clone()),
            Settle::Refused(reason) => (LedgerOutcome::Refused, Some(reason.clone())),
        };
        self.append(id, kind, None, Some(outcome), reason)?;
        self.ledger.insert(id.to_string());
        entry::remove_name(path)?;
        log_settled(self.sid, id, kind, &settle);
        self.report.settled.push((id.to_string(), settle));
        Ok(())
    }

    /// A readable file that is not a valid entry: refused in the ledger when
    /// its kind is still known, then removed.
    fn refuse_malformed(
        &mut self,
        path: &Path,
        id: &str,
        kind: Option<RequestKind>,
        reason: String,
    ) -> Result<()> {
        if let Some(kind) = kind {
            let refused = Some(LedgerOutcome::Refused);
            self.append(id, kind, None, refused, Some(reason.clone()))?;
            self.ledger.insert(id.to_string());
        }
        self.discard(path, id, &reason)
    }

    /// Remove an entry that is refused without being applied or, when its
    /// kind cannot be told, recorded: the ledger has no line for a request
    /// whose kind is unknown.
    fn discard(&mut self, path: &Path, label: &str, reason: &str) -> Result<()> {
        entry::remove_name(path)?;
        tracing::warn!(session_id = %self.sid, entry = %label, %reason, "Refused an inbox entry");
        let refused = Settle::Refused(reason.to_string());
        self.report.settled.push((label.to_string(), refused));
        Ok(())
    }

    fn append(
        &self,
        id: &str,
        kind: RequestKind,
        state: Option<LedgerState>,
        outcome: Option<LedgerOutcome>,
        reason: Option<String>,
    ) -> Result<()> {
        let record = LedgerRecord {
            id: id.to_string(),
            kind,
            state,
            outcome,
            reason,
            at: Utc::now(),
        };
        append_ledger(&self.work_dir, self.sid, &record)
    }
}

/// Every id the ledger holds, after settling each request a previous pass
/// left `applying` with no outcome as `unknown-after-restart`.
fn load_ledger(work_dir: &Path, sid: &str) -> Result<HashSet<String>> {
    let records = read_ledger(work_dir, sid)?;
    let settled: HashSet<&str> = records
        .iter()
        .filter(|record| record.outcome.is_some())
        .map(|record| record.id.as_str())
        .collect();
    let mut interrupted = HashSet::new();
    for record in &records {
        let open =
            record.state == Some(LedgerState::Applying) && !settled.contains(record.id.as_str());
        if open && interrupted.insert(record.id.as_str()) {
            let unknown = LedgerRecord {
                id: record.id.clone(),
                kind: record.kind,
                state: None,
                outcome: Some(LedgerOutcome::UnknownAfterRestart),
                reason: Some(
                    "the daemon stopped while applying this request; it is never applied again"
                        .to_string(),
                ),
                at: Utc::now(),
            };
            append_ledger(work_dir, sid, &unknown)?;
        }
    }
    Ok(records.into_iter().map(|record| record.id).collect())
}

/// Step 4: the entry names this inbox's session, the record's stage, and the
/// id its file is named for.
fn validate_attribution(
    sid: &str,
    record: &Session,
    id: &str,
    inbox_entry: &InboxEntry,
) -> Result<(), String> {
    if inbox_entry.id != id {
        return Err(format!(
            "entry id '{}' does not match its file name '{id}'",
            inbox_entry.id
        ));
    }
    if inbox_entry.session_id != sid {
        return Err(format!(
            "entry names session '{}' but sits in the inbox of session '{sid}'",
            inbox_entry.session_id
        ));
    }
    match record.stage_id.as_deref() {
        Some(stage) if stage == inbox_entry.stage_id => Ok(()),
        Some(stage) => Err(format!(
            "entry names stage '{}' but session '{sid}' works stage '{stage}'",
            inbox_entry.stage_id
        )),
        None => Err(format!(
            "session '{sid}' has no stage to attribute a request to"
        )),
    }
}

/// The request id a `<id>.json` path is named for, when the name is one.
fn file_id(path: &Path) -> Option<String> {
    let id = path.file_stem()?.to_str()?;
    validate_request_id(id).ok()?;
    Some(id.to_string())
}

fn log_settled(sid: &str, id: &str, kind: RequestKind, settle: &Settle) {
    match settle {
        Settle::Applied(note) => {
            tracing::info!(session_id = %sid, request = %id, %kind, note = ?note, "Applied a relayed request")
        }
        Settle::Refused(reason) => {
            tracing::warn!(session_id = %sid, request = %id, %kind, %reason, "Refused a relayed request")
        }
    }
}

/// `W/sessions/<sid>.md`, read without following a symlink. `None` when there
/// is no record.
pub(super) fn load_record(work_dir: &Path, sid: &str) -> Result<Option<Session>> {
    let relative = Path::new("sessions").join(format!("{sid}.md"));
    let content = match read_to_string_bounded(work_dir, &relative, MAX_SESSION_FILE_BYTES) {
        Ok(content) => content,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let session: Session =
        parse_from_markdown(&content, "Session").context("invalid session record")?;
    if session.id != sid {
        bail!("session record '{sid}' names session '{}'", session.id);
    }
    Ok(Some(session))
}

/// An inbox directory with no session record: left alone and reported once,
/// then removed once it has sat untouched for [`ORPHAN_INBOX_SECS`].
fn handle_orphan(host: &mut dyn InboxHost, tick: &Tick<'_>, dir: &Path, sid: &str) {
    let age = std::fs::symlink_metadata(dir)
        .and_then(|metadata| metadata.modified())
        .map(|modified| (tick.now - DateTime::<Utc>::from(modified)).num_seconds())
        .unwrap_or(0);
    if age < ORPHAN_INBOX_SECS {
        if host.first_report(&format!("orphan:{sid}")) {
            tracing::warn!(session_id = %sid, "An inbox directory has no session record; left in place and removed after 24 hours");
        }
        return;
    }
    match entry::remove_tree(dir) {
        Ok(()) => {
            tracing::info!(session_id = %sid, "Removed an inbox directory whose session record never appeared")
        }
        Err(error) => {
            if host.first_report(&format!("orphan-remove:{sid}")) {
                tracing::warn!(session_id = %sid, error = %format!("{error:#}"), "Could not remove an orphaned inbox directory");
            }
        }
    }
}
