//! Recovery: what a server finds in the sessions directory when it starts. A record whose
//! process cannot be verified, or that names another session's socket, is left alone; a dead
//! or provisional one is cleaned; a live pressure session is torn down at once; any other live
//! session gets a final write. The final writes run concurrently on the driver. The driver
//! keeps each live session's record current for a later recovery.

use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::time::Instant;

use super::commands::{Out, Step};
use super::driver::checked_transcript;
use super::ending::END_SLACK;
use super::host::socket_name;
use super::registry::{read_record, remove_session_dir, session_dirs};
use super::registry::{terminate_children, write_record, SessionRecord};
use super::requests::canonical;
use super::session::{Phase, Recovery, Session};
use super::{Bridge, BridgeEvent, EndMode, EndOutcome, RecoveredSession, RecoveryReport};
use crate::fs::safe_read::read_bounded;
use crate::orchestrator::terminal::tmux::socket_path_for;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::DenyReason;
use crate::plan_session::permission::{write_answer, Answer, Behavior};
use crate::plan_session::spool::SpoolReader;
use crate::process::IdentityStatus;

const RESTARTED: &str = "the server restarted";
/// The hook reads answers bounded the same way.
const MAX_ANSWER_BYTES: usize = 4 * 1024;

/// What recovery did with one directory.
enum Found {
    Closing(RecoveredSession),
    TornDown(RecoveredSession),
    LeftAlone(String, String, String),
}

impl Bridge {
    /// Run once before the first chat (`start_session` is Busy until it returns); a later
    /// call returns the first report.
    pub fn recover(&self) -> RecoveryReport {
        if let Some(report) = self.begin_recovery() {
            return report;
        }
        let mut report = RecoveryReport::default();
        let mut closing = Vec::new();
        for (name, dir) in session_dirs(&self.config.runtime.sessions) {
            match self.recover_dir(&name, dir) {
                Found::Closing(session) => closing.push(session),
                Found::TornDown(session) => report.torn_down.push(session),
                Found::LeftAlone(id, socket, why) => report.left_alone.push((id, socket, why)),
            }
        }
        self.flush();
        let ids: Vec<String> = closing.iter().map(|session| session.id.clone()).collect();
        let deadline = Instant::now() + self.config.final_write_bound + END_SLACK;
        self.wait_ended(&ids, deadline);
        report.closed = closing;
        self.finish_recovery(report)
    }

    /// Run `f` with the recovery report once recovery has finished (at once if it has).
    pub fn on_recovered(&self, f: Box<dyn FnOnce(&RecoveryReport) + Send>) {
        let mut inner = self.lock();
        let done = match &inner.recovery {
            Recovery::Done(report) => Some(report.clone()),
            _ => None,
        };
        match done {
            Some(report) => {
                drop(inner);
                f(&report);
            }
            None => inner.waiting.push(f),
        }
    }

    /// Mark recovery running; the report when it already ran (after a concurrent run ends).
    fn begin_recovery(&self) -> Option<RecoveryReport> {
        loop {
            {
                let mut inner = self.lock();
                let state = match &inner.recovery {
                    Recovery::Done(report) => Some(Some(report.clone())),
                    Recovery::NotRun => Some(None),
                    Recovery::Running => None,
                };
                match state {
                    Some(Some(report)) => return Some(report),
                    Some(None) => {
                        inner.recovery = Recovery::Running;
                        return None;
                    }
                    None => {}
                }
            }
            thread::sleep(self.config.tick);
        }
    }

    fn finish_recovery(&self, report: RecoveryReport) -> RecoveryReport {
        let waiting = {
            let mut inner = self.lock();
            inner.recovery = Recovery::Done(report.clone());
            std::mem::take(&mut inner.waiting)
        };
        for f in waiting {
            f(&report);
        }
        self.flush();
        report
    }

    fn recover_dir(&self, name: &str, dir: SessionDir) -> Found {
        let left =
            |socket: &str, why: String| Found::LeftAlone(name.to_owned(), socket.to_owned(), why);
        let record = match read_record(&dir) {
            Ok(record) => record,
            Err(_) if std::fs::symlink_metadata(dir.record()).is_err() => {
                return left("", "no session record".to_owned());
            }
            Err(error) => return left("", format!("unreadable session record: {error:#}")),
        };
        // A planted record must not steer a teardown to another session's tmux server.
        if record.id != name || record.socket != socket_name(name) {
            return left(
                &record.socket,
                "the record names another session".to_owned(),
            );
        }
        let socket_exists = std::fs::symlink_metadata(socket_path_for(&record.socket)).is_ok();
        let status = record
            .identity()
            .map(|identity| self.host.identity(identity));
        let pressure = matches!(record.kind, SessionKind::Pressure { .. });
        match status {
            Some(IdentityStatus::Unverifiable) => {
                let why = "process identity cannot be verified".to_owned();
                left(&record.socket, why)
            }
            Some(IdentityStatus::VerifiedAlive) if !pressure => self.resume(record, dir),
            Some(IdentityStatus::VerifiedAlive) => self.clean(record, &dir, true),
            Some(IdentityStatus::Dead) | None => self.clean(record, &dir, socket_exists),
        }
    }

    /// Tear down (when something may still run), terminate verified children, remove the
    /// directory, report `Ended { TornDown }`.
    fn clean(&self, record: SessionRecord, dir: &SessionDir, teardown: bool) -> Found {
        let left = |why: String| Found::LeftAlone(record.id.clone(), record.socket.clone(), why);
        if teardown {
            if let Err(error) = self.host.teardown(&record.socket) {
                return left(format!("teardown failed: {error:#}"));
            }
        }
        terminate_children(&record.children);
        if let Err(error) = remove_session_dir(dir) {
            return left(format!("cannot remove its directory: {error:#}"));
        }
        let ended = BridgeEvent::Ended {
            id: record.id.clone(),
            kind: record.kind.clone(),
            outcome: EndOutcome::TornDown,
        };
        self.lock().outbox.push(Out::Event(ended));
        Found::TornDown(RecoveredSession {
            id: record.id,
            kind: record.kind,
            children: record.children,
        })
    }

    /// Rebuild the live session from its spool and transcript (the driver reads both from
    /// the start) and run its final write, denying every request on arrival.
    fn resume(&self, record: SessionRecord, dir: SessionDir) -> Found {
        let answers = deny_unanswered(&dir);
        let recovered = RecoveredSession {
            id: record.id.clone(),
            kind: record.kind.clone(),
            children: record.children.clone(),
        };
        let mut session = Session::new(record, dir, self.config.project_root.clone());
        session.phase = Phase::Live;
        session.name = format!("recovered {} session", session.kind.label());
        let recorded = session.record.transcript_path.as_deref();
        session.transcript = recorded.and_then(|path| checked_transcript(path, &session.id));
        session.flow.recovered = Some(answers);
        let restarted = Some((DenyReason::ServerStopping, RESTARTED));
        session.plan_end(EndMode::FinalWrite, restarted);
        let mut inner = self.lock();
        let chat = inner.chat.is_none() && !matches!(session.kind, SessionKind::Summary);
        if chat {
            inner.chat = Some(session.id.clone());
        }
        let mut step = Step::new(chat);
        session.sync(&mut step);
        inner.sessions.insert(session.id.clone(), session);
        inner.outbox.append(&mut step.out);
        Found::Closing(recovered)
    }
}

impl Bridge {
    /// Write the changed record of a live session; the driver is its only writer.
    pub(super) fn record_step(&self, id: &str) {
        let (dir, record) = {
            let mut inner = self.lock();
            let Some(session) = inner.sessions.get_mut(id) else {
                return;
            };
            if !session.record_dirty || !matches!(session.phase, Phase::Live) {
                return;
            }
            session.record_dirty = false;
            (session.dir.clone(), session.record.clone())
        };
        if let Err(error) = write_record(&dir, &record) {
            tracing::warn!(session = %id, %error, "cannot update the session record");
        }
    }
}

/// Deny every spooled request that has no answer file, before the final write (px-state-12),
/// and collect each request's answer so its card shows it.
fn deny_unanswered(dir: &SessionDir) -> HashMap<String, Answer> {
    let mut answers = HashMap::new();
    for event in SpoolReader::new(dir.events()).read_new() {
        if event.event != "PermissionRequest" || event.auto_denied() {
            continue;
        }
        let Some(request_id) = event.request_id.filter(|id| canonical(id)) else {
            continue;
        };
        let answer = read_answer(dir, &request_id).unwrap_or_else(|| {
            let answer = Answer {
                behavior: Behavior::Deny,
                message: Some(RESTARTED.to_owned()),
                reason: Some(DenyReason::ServerStopping),
            };
            if let Err(error) = write_answer(dir.path(), &request_id, &answer) {
                tracing::warn!(%request_id, %error, "cannot deny a request after a restart");
            }
            answer
        });
        answers.insert(request_id, answer);
    }
    answers
}

fn read_answer(dir: &SessionDir, request_id: &str) -> Option<Answer> {
    let name = Path::new("answers").join(format!("{request_id}.json"));
    let bytes = read_bounded(dir.path(), &name, MAX_ANSWER_BYTES).ok()?;
    serde_json::from_slice(&bytes).ok()
}
