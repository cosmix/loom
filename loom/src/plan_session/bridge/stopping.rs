//! Server stops: drain on the first Ctrl-C, abort on the second, and the wait they share with
//! recovery.

use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use super::ending::END_SLACK;
use super::session::{Phase, Session};
use super::{Bridge, BridgeError, EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::DenyReason;

/// How long abort waits for a launch in flight and the teardown after it: the bounds of its
/// tmux calls (spawn 20 s, pane read 5 s, kill-server 5 s).
const LAUNCH_WAIT: Duration = Duration::from_secs(30);

impl Bridge {
    /// First Ctrl-C: final writes for authoring sessions, summary sessions finish their task,
    /// pressure sessions are torn down; returns when all ended or the bound passed.
    pub fn drain(&self) {
        let running: Vec<(String, SessionKind)> = {
            let mut inner = self.lock();
            inner.stopping = true;
            let active = inner.sessions.values().filter(|session| session.active());
            active.map(|s| (s.id.clone(), s.kind.clone())).collect()
        };
        for (id, kind) in &running {
            let mode = match kind {
                SessionKind::Pressure { .. } => EndMode::Teardown,
                _ => EndMode::FinalWrite,
            };
            if let Err(error) = self.end(id, mode, EndCause::Draining) {
                tracing::debug!(session = %id, %error, "drain did not end the session");
            }
        }
        let ids: Vec<String> = running.into_iter().map(|(id, _)| id).collect();
        let deadline = Instant::now() + self.config.final_write_bound + END_SLACK;
        self.wait_ended(&ids, deadline);
    }

    /// Second Ctrl-C: tear every session down now, from the calling thread. A launch in flight
    /// returns on the driver, which tears its session down at once (`end_due`); abort waits for
    /// that, at most `LAUNCH_WAIT`, so no tmux server outlives the exiting server.
    pub fn abort(&self) {
        let ids: Vec<String> = {
            let mut inner = self.lock();
            inner.stopping = true;
            inner.sessions.keys().cloned().collect()
        };
        for id in &ids {
            let claimed = self.with_session(id, |session, step| match session.phase {
                Phase::Live | Phase::Spawning(_) | Phase::TeardownFailed => {
                    Some(session.begin_teardown(DenyReason::ServerStopping, step))
                }
                Phase::Launching => {
                    // Torn down once its launch returns, also under a drain's final write.
                    session.plan_end(EndMode::Teardown, EndCause::Draining, None);
                    None
                }
                _ => None,
            });
            if let Some(Some(teardown)) = claimed {
                self.finish_teardown(id, EndOutcome::TornDown, teardown);
            }
            if let Err(BridgeError::Io(error)) = self.release_session(id) {
                tracing::warn!(session = %id, %error, "cannot release a marker-ended session");
            }
        }
        self.flush();
        self.wait_ended(&ids, Instant::now() + LAUNCH_WAIT);
    }

    /// Wait until none of `ids` is still running and their queued events have been published,
    /// or the deadline. A conclusion removes a session before its `Ended` event is flushed.
    pub(super) fn wait_ended(&self, ids: &[String], deadline: Instant) {
        while Instant::now() < deadline {
            let pending = {
                let inner = self.lock();
                ids.iter()
                    .any(|id| inner.sessions.get(id).is_some_and(Session::active))
                    || !inner.outbox.is_empty()
                    || self.flushing.load(Ordering::SeqCst)
            };
            if !pending {
                return;
            }
            thread::sleep(self.config.tick);
        }
    }
}
