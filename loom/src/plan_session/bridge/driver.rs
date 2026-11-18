//! Tail live spools and transcripts in time order (`Batch::in_time_order`), applying each
//! record in a locked step, then running timers, host and end steps.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::background::Change;
use super::commands::Step;
use super::local::Clock;
use super::registry::{Batch, Entry, Feed, FeedPaths, Feeds};
use super::session::{Closing, EventKey, Phase, Session};
use super::{denied, Bridge, BridgeConfig, BridgeEvent, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{Activity, Item, NoticeKind};
use crate::plan_session::spool::SpoolEvent;
use crate::plan_session::state::{is_idle_prompt, text, Input};
use crate::plan_session::transcript::RecordMark;

impl Bridge {
    pub(super) fn tick(&self, feeds: &mut Feeds) {
        self.spawn_pending();
        let ids: Vec<String> = self.lock().sessions.keys().cloned().collect();
        for id in &ids {
            self.tick_session(id, feeds);
        }
        let inner = self.lock();
        feeds.retain(|id, _| {
            let session = inner.sessions.get(id);
            session.is_some_and(|session| matches!(session.phase, Phase::Live))
        });
        drop(inner);
        self.flush();
    }

    fn tick_session(&self, id: &str, feeds: &mut Feeds) {
        if let Some(paths) = self.feed_paths(id) {
            let events = paths.events.clone();
            let feed = feeds
                .entry(id.to_owned())
                .or_insert_with(|| Feed::new(events));
            feed.sync(&paths);
            let batch = feed.read();
            self.apply_batch(id, batch);
        }
        let stall_budget = self.lock().stall_budget;
        self.step(id, |session, config, step| {
            session.timers(config, stall_budget, step)
        });
        self.marker_step(id);
        self.identity_step(id);
        self.end_step(id);
        self.press_step(id);
        self.deliver_step(id);
        self.record_step(id);
        self.flush();
    }

    fn feed_paths(&self, id: &str) -> Option<FeedPaths> {
        let inner = self.lock();
        let session = inner.sessions.get(id)?;
        matches!(session.phase, Phase::Live).then(|| FeedPaths {
            events: session.dir.events(),
            transcript: session.transcript.clone(),
            tokens: session.token_paths.clone(),
        })
    }

    fn apply_batch(&self, id: &str, batch: Batch) {
        for entry in batch.in_time_order() {
            self.step(id, |session, config, step| match entry {
                Entry::Main(line) => session.ingest_main(line, step),
                Entry::Agent(agent, by, line) => session.ingest_agent(agent, by, line, step),
                Entry::Event(event) => session.apply_event(event, config, step),
            });
        }
        // After the lines: a meta file read with them says what holds once they were written.
        for (agent, meta) in &batch.metas {
            self.step(id, |session, _, step| {
                let change = session.background.meta(agent, meta);
                session.publish_background(change, step);
            });
        }
        for line in &batch.tokens {
            self.step(id, |session, _, step| {
                let deltas = session.mapper.ingest_tokens_only(line);
                session.emit(deltas, step);
            });
        }
        for (agent, name, lines) in &batch.teammates {
            self.step(id, |session, config, step| {
                session.ingest_teammate(agent, name, lines, config, step);
            });
        }
    }
}

impl Session {
    fn ingest_teammate(
        &mut self,
        agent: &str,
        name: &str,
        lines: &[String],
        config: &BridgeConfig,
        step: &mut Step,
    ) {
        let tracker = self
            .team
            .get_or_insert_with(|| super::team::TeamTracker::new(config.project_root.clone()));
        if tracker.ingest(agent, name, lines) {
            step.event(BridgeEvent::Team {
                id: self.id.clone(),
                teammates: tracker.statuses(),
            });
        }
    }

    /// One spool event; an event already applied (a replayed spool) changes nothing.
    pub(super) fn apply_event(
        &mut self,
        event: &SpoolEvent,
        config: &BridgeConfig,
        step: &mut Step,
    ) {
        if !self.flow.applied.insert(event_key(event)) {
            return;
        }
        // The table first: a Stop's list ends tasks before the Stop's close gate reads it.
        let change = self.background.event(event);
        self.publish_background(change, step);
        let at = event.received_at;
        // Idle evidence loses to a main record seen later (the machine's own rule).
        let wins = self.machine.last_main_at().is_none_or(|main| main <= at);
        self.machine.apply(Input::Spool(event));
        match event.event.as_str() {
            "SessionStart" => self.on_session_start(event, step),
            "UserPromptSubmit" => self.on_prompt(at, text(&event.payload, "prompt"), step),
            "PermissionRequest" => self.on_request(event, config, step),
            "PermissionDenied" => self.notice(
                NoticeKind::Info,
                &denied::denied_notice(&event.payload),
                step,
            ),
            "Notification" if wins && is_idle_prompt(&event.payload) => self.on_idle(false, step),
            "Stop" | "StopFailure" if wins => {
                if event.event == "Stop" {
                    self.agent_end();
                }
                self.on_idle(true, step);
            }
            "PreCompact" => self.on_pre_compact(&event.payload, step),
            "SessionEnd" if self.flow.close_ready.is_none() => {
                let confirmed = self.closing() == Some(Closing::Confirmed);
                let outcome = if confirmed {
                    EndOutcome::Closed
                } else {
                    EndOutcome::TornDown
                };
                self.flow.close_ready = Some(outcome);
            }
            _ => {}
        }
    }

    fn on_session_start(&mut self, event: &SpoolEvent, step: &mut Step) {
        if let Some(path) = event.transcript_path.as_deref() {
            match checked_transcript(path, &self.id) {
                Some(path) if self.transcript.as_ref() != Some(&path) => {
                    self.record.transcript_path = Some(path.clone());
                    self.record_dirty = true;
                    self.transcript = Some(path);
                }
                Some(_) => {}
                None => tracing::warn!(session = %self.id, path = %path.display(),
                    "ignoring a transcript path that is not this session's"),
            }
        }
        // A delivered `/compact` is confirmed and settles from here (px-state-10).
        if text(&event.payload, "source") == Some("compact") {
            self.local_record("compact", None, Clock::Boundary, step);
        }
    }

    fn on_prompt(&mut self, at: DateTime<Utc>, prompt: Option<&str>, step: &mut Step) {
        self.confirm_prompt(prompt, step);
        if self.flow.turn.is_some() && !self.flow.prompted {
            // The records of this prompt arrived first and already opened its turn.
            self.flow.prompted = true;
        } else {
            self.begin_turn(at, step);
            self.flow.prompted = true;
        }
    }

    /// A Stop or StopFailure, an idle prompt or an interrupt record that won: the turn is over.
    pub(super) fn on_idle(&mut self, stop: bool, step: &mut Step) {
        self.settle_now();
        self.end_turn(step);
        self.flow.paused = false;
        if self.flow.quiet_until.take().is_some() {
            self.flow.skip_delivery = true;
        }
        if stop {
            // A summary session still owes a turn for its agents' hand-backs, so only a
            // Stop with the background table already drained latches its close.
            let drained =
                !matches!(self.kind, SessionKind::Summary) || self.background.blocking() == 0;
            self.flow.closing_stopped = self.closing_done() && drained;
        }
        self.check_close();
    }

    fn closing_done(&self) -> bool {
        let closing_done = matches!(self.closing(), Some(Closing::Confirmed | Closing::Skip));
        match self.kind {
            SessionKind::Pressure { .. } => false,
            SessionKind::Summary => self.flow.opening.is_none() && self.flow.delivering.is_none(),
            SessionKind::Authoring => {
                closing_done
                    && self
                        .flow
                        .end
                        .as_ref()
                        .is_some_and(|plan| plan.mode == EndMode::FinalWrite)
            }
        }
    }

    /// Closing is a state check: background work can drain after the main Stop.
    fn check_close(&mut self) {
        if self.flow.closing_stopped
            && self.closing_done()
            && matches!(self.phase, Phase::Live)
            && self.machine.activity() == Activity::Idle
            && self.flow.opening.is_none()
            && self.flow.delivering.is_none()
            && self.background.blocking() == 0
            && self.flow.close_ready.is_none()
        {
            self.flow.close_ready = Some(EndOutcome::Closed);
        }
    }

    pub(super) fn closing(&self) -> Option<Closing> {
        self.flow.end.as_ref().map(|plan| plan.closing)
    }

    /// One `SubagentStopped` per agent id that ended for the first time, at once; the table
    /// itself goes out once per tick (`timers`), however many lines changed it.
    fn publish_background(&mut self, change: Change, step: &mut Step) {
        self.flow.background_changed |= change.changed;
        for agent_id in change.stopped {
            let id = self.id.clone();
            step.event(BridgeEvent::SubagentStopped { id, agent_id });
        }
    }

    pub(super) fn ingest_main(&mut self, line: &str, step: &mut Step) {
        let ingest = self.mapper.ingest_main(line);
        self.emit(ingest.deltas, step);
        for mark in ingest.marks {
            self.apply_mark(mark, step);
        }
    }

    fn apply_mark(&mut self, mark: RecordMark, step: &mut Step) {
        let change = self.background.mark(&mark);
        self.publish_background(change, step);
        match mark {
            RecordMark::Main { at } => {
                self.settle_now();
                let fresh = self.after_idle(at) && self.flow.delivering.is_none();
                self.machine.apply(Input::Record { at, main: true });
                self.begin_unprompted(fresh, at, step);
            }
            RecordMark::SelfStartedTurn { at } => {
                // Hand-backs after the final Stop update the table without opening a turn.
                // A real prompt or main record still starts work and holds the close gate.
                if self.flow.closing_stopped && self.flow.turn.is_none() {
                    return;
                }
                self.settle_now();
                let fresh = self.after_idle(at);
                self.machine.apply(Input::SelfStartedTurn { at });
                self.begin_unprompted(fresh, at, step);
            }
            RecordMark::Interrupted { at } => {
                self.machine.apply(Input::Interrupted { at });
                self.on_idle(false, step);
            }
            // The table read both: a killed agent and a task's evidence move no session state.
            RecordMark::AgentsKilled { .. } | RecordMark::Background { .. } => {}
            RecordMark::CompactBoundary { .. } => {
                self.local_record("compact", None, Clock::Boundary, step);
            }
            RecordMark::HumanUser { text, .. } => self.confirm_text(&text, step),
            RecordMark::Command(mark) => self.on_command(mark, step),
        }
    }

    fn after_idle(&self, at: DateTime<Utc>) -> bool {
        self.machine.last_idle_at().is_none_or(|idle| at > idle)
    }

    /// A turn Claude Code started without a prompt opens through the prompt path (px-state-08).
    fn begin_unprompted(&mut self, fresh: bool, at: DateTime<Utc>, step: &mut Step) {
        if fresh && self.flow.turn.is_none() {
            self.begin_turn(at, step);
            self.flow.prompted = false;
        }
    }

    pub(super) fn ingest_agent(
        &mut self,
        agent: &str,
        by: Option<&str>,
        line: &str,
        step: &mut Step,
    ) {
        // Every line reaches the table, also one that maps to no chat item.
        if let Ok(entry) = serde_json::from_str::<Value>(line) {
            let change = self.background.agent_line(agent, &entry);
            self.publish_background(change, step);
        }
        let ingest = self.mapper.ingest_subagent(agent, by, line);
        self.emit(ingest.deltas, step);
    }

    pub(super) fn timers(
        &mut self,
        config: &BridgeConfig,
        stall_budget: Duration,
        step: &mut Step,
    ) {
        if !matches!(self.phase, Phase::Live) {
            return;
        }
        let change = self.background.reevaluate(Utc::now(), stall_budget);
        self.publish_background(change, step);
        if std::mem::take(&mut self.flow.background_changed) {
            let (id, tasks) = (self.id.clone(), self.background.tasks());
            step.event(BridgeEvent::Background { id, tasks });
        }
        let now = Instant::now();
        self.expire_requests(step);
        self.time_out_delivery(config, now, step);
        if self.flow.quiet_until.is_some_and(|until| now >= until) {
            self.flow.quiet_until = None;
        }
        self.settle_due(now, step);
        self.check_close();
    }

    pub(super) fn notice(&mut self, kind: NoticeKind, text: &str, step: &mut Step) {
        let id = self.next_id("n-s");
        let text = text.to_owned();
        let deltas = self.mapper.add_server_item(Item::Notice { id, kind, text });
        self.emit(deltas, step);
    }
}

/// Identifies one spool line across a replayed spool: two distinct events in the same
/// millisecond (two `SubagentStart`s with no request id) differ in their payload.
fn event_key(event: &SpoolEvent) -> EventKey {
    let mut hasher = DefaultHasher::new();
    event.payload.to_string().hash(&mut hasher);
    let payload = hasher.finish();
    let request = event.request_id.clone();
    (event.received_at, event.event.clone(), request, payload)
}

/// An event the bridge itself applies; it never enters the applied set.
pub(super) fn synthetic(event: &str, payload: Value) -> SpoolEvent {
    SpoolEvent {
        received_at: Utc::now(),
        event: event.to_owned(),
        session_id: None,
        transcript_path: None,
        request_id: None,
        payload,
    }
}

/// An untrusted `SessionStart` path must be absolute, have plain components and name our
/// Claude Code session's `<id>.jsonl`. Reads refuse symlinked or non-regular files.
pub(super) fn checked_transcript(path: &Path, id: &str) -> Option<PathBuf> {
    let plain = path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)));
    let expected = format!("{id}.jsonl");
    let named = path.file_name().and_then(|name| name.to_str()) == Some(expected.as_str());
    (plain && named).then(|| path.to_path_buf())
}
