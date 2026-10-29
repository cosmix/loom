//! The running session: its start settling, claims, seals, stall, end and the server stop.

use std::collections::BTreeMap;
use std::time::Instant;

use super::{Running, Scheduler, SchedulerEvent, SessionSpawner, StopReason, Via};
use crate::planning::summary::cost::Trigger;

/// Why a summary fails when a second normal end leaves it claimed but unsealed.
pub(super) const ENDED_UNSEALED: &str = "the summary session ended twice without sealing it";

impl Scheduler {
    pub fn running_session(&self) -> Option<&str> {
        self.running
            .as_ref()
            .map(|running| running.session.as_str())
    }

    /// `(sealed, total)` of the running session.
    pub fn progress(&self) -> Option<(usize, usize)> {
        let running = self.running.as_ref()?;
        Some((running.sealed.len(), running.ids.len()))
    }

    /// What queued each id of the running session: a Button, else a batch.
    pub fn running_triggers(&self) -> BTreeMap<String, Trigger> {
        let Some(running) = &self.running else {
            return BTreeMap::new();
        };
        let trigger = |id: &String| match running.vias.get(id) {
            Some(Via::Button) => Trigger::Button,
            _ => Trigger::Background,
        };
        running
            .ids
            .iter()
            .map(|id| (id.clone(), trigger(id)))
            .collect()
    }

    /// Settle a start made through a spawner that answered `pending` for the real one: the
    /// session runs under `result`'s id, or on an error nothing runs, its ids return to the
    /// queue and the panel shows why. Returns the started session's id.
    pub fn resolve_start(
        &mut self,
        pending: &str,
        result: anyhow::Result<String>,
    ) -> Option<String> {
        let running = self.running.as_mut().filter(|r| r.session == pending)?;
        match result {
            Ok(session) => {
                running.session.clone_from(&session);
                Some(session)
            }
            Err(error) => {
                self.start_error = Some(format!("{error:#}"));
                let running = self.running.take()?;
                self.requeue(running.ids, &running.vias);
                None
            }
        }
    }

    /// The running session's claims: once every id was claimed or sealed and none is held,
    /// stop it (the work is sealed or released). Claims arrive in waves (documents, then
    /// overviews), so one empty listing with ids never claimed is a gap, not the end.
    pub fn claims_changed(
        &mut self,
        session: &str,
        held: &[String],
        _now: Instant,
        spawner: &dyn SessionSpawner,
    ) {
        let Some(running) = self.running.as_mut().filter(|r| r.session == session) else {
            return;
        };
        running.claimed.extend(held.iter().cloned());
        let settled = running
            .ids
            .iter()
            .all(|id| running.claimed.contains(id) || running.sealed.contains(id));
        if held.is_empty() && settled && !running.stopping {
            running.stopping = true;
            spawner.stop(session, false);
            let (session, reason) = (session.to_owned(), StopReason::ClaimsDone);
            self.out.push(SchedulerEvent::Stopped { session, reason });
        }
    }

    /// A summary of the running session was sealed: count it and reset the stall timer.
    pub fn seal_seen(&mut self, id: &str, now: Instant) {
        let runs_it = |r: &&mut Running| r.ids.iter().any(|own| own == id);
        if let Some(running) = self.running.as_mut().filter(runs_it) {
            running.sealed.insert(id.to_owned());
            running.since = now;
            self.unsealed.remove(id);
        }
    }

    /// The error a stall of `session` reports, read from its last failed seal.
    pub fn note_seal_error(&mut self, session: &str, error: Option<String>) {
        if let Some(running) = self.running.as_mut().filter(|r| r.session == session) {
            running.seal_error = error;
        }
    }

    /// The running session when the next tick stops it for a stall.
    pub fn stall_due(&self, now: Instant) -> Option<&str> {
        let running = self.running.as_ref()?;
        let quiet = now.saturating_duration_since(running.since);
        let due = !running.stopping && quiet >= self.config.no_seal_timeout;
        due.then_some(running.session.as_str())
    }

    /// The session ended: its unsealed ids go back to the queue; then try a start.
    pub fn session_ended(&mut self, session: &str, now: Instant, spawner: &dyn SessionSpawner) {
        self.end_session(session, None, now, spawner);
    }

    /// The session ended with `reason`: its unsealed ids fail with it; then try a start.
    pub fn session_failed(
        &mut self,
        session: &str,
        reason: &str,
        now: Instant,
        spawner: &dyn SessionSpawner,
    ) {
        self.end_session(session, Some(reason), now, spawner);
    }

    /// Stop a session that sealed nothing for the timeout, releasing its claims; its unsealed
    /// ids fail with the last seal error.
    pub fn tick(&mut self, now: Instant, spawner: &dyn SessionSpawner) -> Vec<SchedulerEvent> {
        if self.stall_due(now).is_some() {
            self.stall(spawner);
        }
        self.take_events()
    }

    /// The server is stopping: drop the queue and the question, start nothing more. The
    /// session bridge's drain ends a running session, so it is reported, not stopped.
    pub fn server_stopping(&mut self) {
        self.stopping = true;
        self.queue.clear();
        if let Some(open) = self.question.take() {
            self.out
                .push(SchedulerEvent::QuestionClosed { id: open.id });
        }
        if let Some(running) = &self.running {
            let (session, reason) = (running.session.clone(), StopReason::ServerStopping);
            self.out.push(SchedulerEvent::Stopped { session, reason });
        }
    }

    /// The session is gone: its unsealed ids return to the queue, or fail with `failure`.
    fn end_session(
        &mut self,
        session: &str,
        failure: Option<&str>,
        now: Instant,
        spawner: &dyn SessionSpawner,
    ) {
        let Some(running) = self.running.take_if(|r| r.session == session) else {
            return;
        };
        let unsealed: Vec<String> = running
            .ids
            .iter()
            .filter(|id| !running.sealed.contains(*id) && !self.failed.contains_key(*id))
            .cloned()
            .collect();
        let reason = failure.map_or(StopReason::Ended, |why| StopReason::Failed(why.to_owned()));
        let session = session.to_owned();
        self.out.push(SchedulerEvent::Stopped { session, reason });
        match failure {
            Some(why) => self.fail(unsealed, why),
            None => self.requeue_unsealed(unsealed, &running),
        }
        self.try_start(now, spawner);
    }

    /// After a normal end, requeue the unsealed ids. An id the session claimed was worked on
    /// and released without a seal; when the previous normal end did the same to it, it fails
    /// instead (a summary whose seal never validates would otherwise rebuild forever, at a
    /// token cost every round). An id the session never claimed (it ended before reaching it)
    /// requeues and keeps its mark.
    fn requeue_unsealed(&mut self, unsealed: Vec<String>, running: &Running) {
        let worked = |id: &String| running.claimed.contains(id);
        let (twice, again): (Vec<String>, Vec<String>) = unsealed
            .into_iter()
            .partition(|id| worked(id) && self.unsealed.contains(id));
        self.unsealed
            .extend(again.iter().filter(|id| worked(id)).cloned());
        self.fail(twice, ENDED_UNSEALED);
        self.requeue(again, &running.vias);
    }

    fn stall(&mut self, spawner: &dyn SessionSpawner) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        running.stopping = true;
        let unsealed: Vec<String> = running
            .ids
            .iter()
            .filter(|id| !running.sealed.contains(*id))
            .cloned()
            .collect();
        let minutes = self.config.no_seal_timeout.as_secs() / 60;
        let fallback = || format!("no seal in {minutes} minutes");
        let error = running.seal_error.clone().unwrap_or_else(fallback);
        let session = running.session.clone();
        spawner.stop(&session, true);
        let reason = StopReason::Stalled;
        self.out.push(SchedulerEvent::Stopped { session, reason });
        self.fail(unsealed, &error);
    }

    fn fail(&mut self, ids: Vec<String>, error: &str) {
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.failed.insert(id.clone(), error.to_owned());
            self.unsealed.remove(id);
        }
        let error = error.to_owned();
        self.out.push(SchedulerEvent::Failed { ids, error });
    }
}
