//! Input the bridge accepted and the session has not confirmed, kept in `pending.json` next to
//! the record. A server that dies loses its memory; the next one's recovery reads the file and
//! returns the texts to the holder (`BridgeEvent::InputReturned`) instead of replaying them.
//!
//! No filesystem call happens under the bridge lock. `pending_io` orders the writes: it is
//! taken first, the bridge lock inside it for short copies only.

use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::session::{Phase, Session};
use super::{guard, Bridge, BridgeError, SendOutcome};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::fs::safe_replace::replace_private;
use crate::plan_session::attachments::{parse_unwrapped, Attachment};
use crate::plan_session::input::{check_delivered, sanitize};
use crate::plan_session::kind::SessionDir;

const PENDING: &str = "pending.json";
/// 21 messages of `MAX_DELIVERED_BYTES` fit with JSON escaping.
const MAX_PENDING_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct PendingInput {
    texts: Vec<String>,
}

fn write_pending(dir: &SessionDir, texts: &[String]) -> anyhow::Result<()> {
    let input = PendingInput {
        texts: texts.to_vec(),
    };
    let bytes = serde_json::to_vec(&input)?;
    replace_private(dir.path(), Path::new(PENDING), &bytes)
        .with_context(|| format!("cannot write {}", dir.path().join(PENDING).display()))
}

/// The texts `pending.json` holds; none when it is absent or unreadable (a warning for the
/// latter).
pub(super) fn read_pending(dir: &SessionDir) -> Vec<String> {
    let read = read_bounded(dir.path(), Path::new(PENDING), MAX_PENDING_BYTES);
    let bytes = match read {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Vec::new(),
        Err(error) => {
            tracing::warn!(dir = %dir.path().display(), %error, "cannot read the pending input");
            return Vec::new();
        }
    };
    match serde_json::from_slice::<PendingInput>(&bytes) {
        Ok(input) => input.texts,
        Err(error) => {
            tracing::warn!(dir = %dir.path().display(), %error, "cannot parse the pending input");
            Vec::new()
        }
    }
}

impl Session {
    /// The user texts a restart would lose, oldest first: the message being delivered, then the
    /// queue. An opening (the start's first message) is not covered: the draft keeps its
    /// kickoff text. A closing message and a local command already confirmed are not input.
    pub(super) fn pending_texts(&self) -> Vec<String> {
        let delivering = self.flow.delivering.as_ref().filter(|d| {
            let confirmed = d.local.as_ref().is_some_and(|l| l.confirmed_at.is_some());
            !d.closing && d.opening.is_none() && !confirmed
        });
        let delivering = delivering.map(|d| parse_unwrapped(&d.text).0);
        let queued = self
            .flow
            .queue
            .iter()
            .map(|queued| queued.user_text.clone());
        delivering.into_iter().chain(queued).collect()
    }
}

impl Bridge {
    /// `text` is the formatted message (attachment block included); `attachments` its chips.
    /// The message is on disk before it is accepted: a failed save refuses it.
    pub fn send(
        &self,
        id: &str,
        text: String,
        attachments: Vec<Attachment>,
    ) -> Result<SendOutcome, BridgeError> {
        let text = sanitize(&text);
        check_delivered(&text).map_err(BridgeError::Input)?;
        let io = guard(&self.pending_io);
        let (dir, mut prospective) = {
            let inner = self.lock();
            let session = inner.sessions.get(id).ok_or(BridgeError::NoSession)?;
            if !session.accepts_input() {
                return Err(BridgeError::NotRunning);
            }
            (session.dir.clone(), session.pending_texts())
        };
        prospective.push(parse_unwrapped(&text).0);
        if let Err(error) = write_pending(&dir, &prospective) {
            let why = format!("the message could not be saved: {error:#}");
            return Err(BridgeError::Io(why));
        }
        let sent = self.with_session(id, |session, step| {
            session.flow.pending_saved = prospective;
            session.send(text, attachments, step)
        });
        self.save_pending_held(id);
        drop(io);
        self.flush();
        sent.unwrap_or(Err(BridgeError::NoSession))
    }

    /// Bring the file up to the session's pending texts (a withdrawal, an interrupt, a
    /// confirmed delivery). A failed write is retried by the next call.
    pub(super) fn save_pending(&self, id: &str) {
        let _io = guard(&self.pending_io);
        self.save_pending_held(id);
    }

    /// The caller holds `pending_io`.
    fn save_pending_held(&self, id: &str) {
        let (dir, texts) = {
            let inner = self.lock();
            let Some(session) = inner.sessions.get(id) else {
                return;
            };
            let live = matches!(
                session.phase,
                Phase::Spawning(_) | Phase::Launching | Phase::Live
            );
            let texts = session.pending_texts();
            if !live || texts == session.flow.pending_saved {
                return;
            }
            (session.dir.clone(), texts)
        };
        if let Err(error) = write_pending(&dir, &texts) {
            tracing::warn!(session = %id, %error, "cannot save the pending input");
            return;
        }
        if let Some(session) = self.lock().sessions.get_mut(id) {
            session.flow.pending_saved = texts;
        }
    }
}
