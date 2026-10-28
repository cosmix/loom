//! The driver tick: tail each live session's spool and transcripts, apply what arrived one
//! line or event per locked step, run the timers, then the host and end steps.
//!
//! A tick applies a session's records and hook events in time order (`Batch::in_time_order`).

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::commands::Step;
use super::registry::{Batch, Entry, Feed, FeedPaths, Feeds};
use super::session::{Closing, EventKey, Phase, Session};
use super::{Bridge, BridgeConfig, BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{Item, NoticeKind};
use crate::plan_session::spool::SpoolEvent;
use crate::plan_session::state::{is_idle_prompt, text, Input};
use crate::plan_session::transcript::RecordMark;

/// A delivered `/compact` whose boundary record had no Stop after it for this long is idle.
const COMPACT_SETTLE: Duration = Duration::from_secs(2);

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
        let stall = self.lock().stall_after;
        self.step(id, |session, config, step| {
            session.timers(config, stall, step)
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
        for line in &batch.tokens {
            self.step(id, |session, _, step| {
                let deltas = session.mapper.ingest_tokens_only(line);
                session.emit(deltas, step);
            });
        }
    }
}

impl Session {
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
        let at = event.received_at;
        // Idle evidence loses to a main record seen later (the machine's own rule).
        let wins = self.machine.last_main_at().is_none_or(|main| main <= at);
        if event.event == "SubagentStop" {
            self.on_agent_stop(event, step);
            return;
        }
        self.machine.apply(Input::Spool(event));
        match event.event.as_str() {
            "SessionStart" => self.on_session_start(event, step),
            "UserPromptSubmit" => self.on_prompt(at, text(&event.payload, "prompt"), step),
            "PermissionRequest" => self.on_request(event, config, step),
            "Notification" if wins && is_idle_prompt(&event.payload) => self.on_idle(false, step),
            "Stop" if wins => self.on_idle(true, step),
            "SubagentStart" => {
                if let Some(agent) = text(&event.payload, "agent_id") {
                    self.flow.agents.entry(agent.to_owned()).or_default();
                }
            }
            "PreCompact" => self.notice(NoticeKind::Info, "Compacting the context", step),
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
        // A delivered `/compact` is confirmed and ends here (px-state-10).
        if text(&event.payload, "source") == Some("compact") && self.flow.compact.is_some() {
            self.flow.compact = None;
            self.confirm_compact(step);
            self.end_turn(step);
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

    /// A Stop, an idle prompt or an interrupt record that won: the turn is over.
    fn on_idle(&mut self, stop: bool, step: &mut Step) {
        self.end_turn(step);
        self.flow.paused = false;
        self.flow.compact = None;
        if self.flow.quiet_until.take().is_some() {
            self.flow.skip_delivery = true;
        }
        if stop && self.machine.subagents_running() == 0 && self.closes_at_stop() {
            self.flow.close_ready = Some(EndOutcome::Closed);
        }
    }

    fn closes_at_stop(&self) -> bool {
        match (&self.kind, self.closing()) {
            (SessionKind::Pressure { .. }, _) => false,
            (_, Some(Closing::Confirmed)) => true,
            // A summary session ends at the first Stop after its task was delivered.
            (SessionKind::Summary, None | Some(Closing::Skip)) => {
                self.flow.opening.is_none() && self.flow.delivering.is_none()
            }
            _ => false,
        }
    }

    pub(super) fn closing(&self) -> Option<Closing> {
        self.flow.end.as_ref().map(|plan| plan.closing)
    }

    fn on_agent_stop(&mut self, event: &SpoolEvent, step: &mut Step) {
        let agent = text(&event.payload, "agent_id").map(str::to_owned);
        if let Some(agent) = &agent {
            let watch = self.flow.agents.entry(agent.clone()).or_default();
            if watch.stopped {
                return;
            }
            watch.stopped = true;
        }
        self.machine.apply(Input::Spool(event));
        if let Some(agent_id) = agent {
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
        match mark {
            RecordMark::Main { at } => {
                let fresh = self.after_idle(at) && self.flow.delivering.is_none();
                self.machine.apply(Input::Record { at, main: true });
                self.begin_unprompted(fresh, at, step);
            }
            RecordMark::SelfStartedTurn { at } => {
                let fresh = self.after_idle(at);
                self.machine.apply(Input::SelfStartedTurn { at });
                self.begin_unprompted(fresh, at, step);
            }
            RecordMark::Interrupted { at } => {
                self.machine.apply(Input::Interrupted { at });
                self.on_idle(false, step);
            }
            RecordMark::AgentsKilled { at } => {
                self.machine.apply(Input::AgentsKilled { at });
            }
            RecordMark::CompactBoundary { .. } => {
                self.confirm_compact(step);
                if let Some(seen) = &mut self.flow.compact {
                    *seen = Some(Instant::now());
                }
            }
            RecordMark::HumanUser { text, .. } => self.confirm_text(&text, step),
            RecordMark::SubagentFinal { .. } => {}
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
        let ingest = self.mapper.ingest_subagent(agent, by, line);
        if ingest.deltas.is_empty() && ingest.marks.is_empty() {
            return;
        }
        let last_final = ingest
            .marks
            .iter()
            .any(|mark| matches!(mark, RecordMark::SubagentFinal { .. }));
        self.emit(ingest.deltas, step);
        let watch = self.flow.agents.entry(agent.to_owned()).or_default();
        watch.final_at = last_final.then(Instant::now);
    }

    pub(super) fn timers(&mut self, config: &BridgeConfig, stall: Duration, step: &mut Step) {
        if !matches!(self.phase, Phase::Live) {
            return;
        }
        let now = Instant::now();
        self.expire_requests(step);
        self.time_out_delivery(config, now, step);
        if self.flow.quiet_until.is_some_and(|until| now >= until) {
            self.flow.quiet_until = None;
        }
        let settled = self.flow.compact.flatten();
        if settled.is_some_and(|seen| now.duration_since(seen) >= COMPACT_SETTLE) {
            self.flow.compact = None;
            let idle = synthetic("Notification", json!({"notification_type": "idle_prompt"}));
            self.machine.apply(Input::Spool(&idle));
            self.on_idle(false, step);
        }
        self.stop_stalled(stall, now, step);
    }

    /// A finished subagent transcript that stopped growing gets one synthetic SubagentStop.
    fn stop_stalled(&mut self, stall: Duration, now: Instant, step: &mut Step) {
        let stalled: Vec<String> = self
            .flow
            .agents
            .iter()
            .filter(|(_, watch)| !watch.stopped)
            .filter(|(_, watch)| {
                watch
                    .final_at
                    .is_some_and(|at| now.duration_since(at) >= stall)
            })
            .map(|(agent, _)| agent.clone())
            .collect();
        for agent in stalled {
            let stop = synthetic("SubagentStop", json!({ "agent_id": agent }));
            self.on_agent_stop(&stop, step);
        }
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
fn synthetic(event: &str, payload: Value) -> SpoolEvent {
    SpoolEvent {
        received_at: Utc::now(),
        event: event.to_owned(),
        session_id: None,
        transcript_path: None,
        request_id: None,
        payload,
    }
}

/// The transcript a `SessionStart` names, when it is safe to tail. The payload is untrusted:
/// the path must be absolute with plain components and name `<this session's id>.jsonl` (the
/// launch makes our id Claude Code's session id). The reads refuse a symlinked or
/// non-regular file.
pub(super) fn checked_transcript(path: &Path, id: &str) -> Option<PathBuf> {
    let plain = path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)));
    let expected = format!("{id}.jsonl");
    let named = path.file_name().and_then(|name| name.to_str()) == Some(expected.as_str());
    (plain && named).then(|| path.to_path_buf())
}
