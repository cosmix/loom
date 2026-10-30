//! How a session publishes itself: its view, state events, turns and queue.

use chrono::Utc;

use super::commands::{Out, Step};
use super::session::{stamp, Queued, Session};
use super::BridgeEvent;
use crate::plan_session::model::{
    ChatDelta, EndedChat, QueuedMessage, SessionState, SessionView, Turn,
};

/// A queued message's first line in the queue list.
const FIRST_LINE_CHARS: usize = 120;

impl Session {
    pub(super) fn view(&self) -> SessionView {
        SessionView {
            id: self.id.clone(),
            kind: self.kind.label().to_owned(),
            state: self.state(),
            name: self.name.clone(),
            model: self.model.clone(),
            socket: self.record.socket.clone(),
            holder: None,
            away_until: None,
            queued: u32::try_from(self.flow.queue.len()).unwrap_or(u32::MAX),
            started_at: self.record.started_at.clone(),
            state_since: self.shown.since.clone(),
            working_since: self.shown.working_since.clone(),
            subagents_running: self.machine.subagents_running(),
            last_activity: self.mapper.last_main_record_at().map(stamp),
            failure: self.failure.clone(),
            offer: self.offer.clone(),
            feature: self.record.feature.clone(),
            plan: self.record.plan.clone(),
        }
    }

    /// Publish a state change as an event and a changed view as a `Session` delta.
    pub(super) fn sync(&mut self, step: &mut Step) {
        let state = self.state();
        if state != self.shown.state {
            let now = stamp(Utc::now());
            let busy = |state: SessionState| {
                matches!(state, SessionState::Working | SessionState::Permission)
            };
            if !busy(state) {
                self.shown.working_since = None;
            } else if !busy(self.shown.state) {
                self.shown.working_since = Some(now.clone());
            }
            self.shown.state = state;
            self.shown.since = now;
            let id = self.id.clone();
            step.event(BridgeEvent::State { id, state });
        }
        if step.chat {
            let view = self.view();
            if self.shown.view.as_ref() != Some(&view) {
                let session = view.clone();
                step.out
                    .push(Out::Delta(Box::new(ChatDelta::Session { session })));
                self.shown.view = Some(view);
            }
        }
    }

    /// Publish mapper deltas, with the bridge's changed files on every turn update.
    pub(super) fn emit(&self, deltas: Vec<ChatDelta>, step: &mut Step) {
        if !step.chat {
            return;
        }
        for mut delta in deltas {
            if let ChatDelta::TurnUpdated {
                turn_id,
                changed_files,
                ..
            } = &mut delta
            {
                if let Some(changed) = self.flow.changed.get(turn_id.as_str()) {
                    changed_files.clone_from(changed);
                }
            }
            step.out.push(Out::Delta(Box::new(delta)));
        }
    }

    pub(super) fn publish_queue(&self, step: &mut Step) {
        if step.chat {
            let queue = self.queue_view(true);
            step.out
                .push(Out::Delta(Box::new(ChatDelta::Queue { queue })));
        }
    }

    pub(super) fn turns(&self) -> Vec<Turn> {
        let mut turns = self.mapper.turns().to_vec();
        for turn in &mut turns {
            if let Some(changed) = self.flow.changed.get(&turn.id) {
                turn.changed_files.clone_from(changed);
            }
        }
        turns
    }

    pub(super) fn queue_view(&self, for_holder: bool) -> Vec<QueuedMessage> {
        let entry = |queued: &Queued| QueuedMessage {
            id: queued.id.clone(),
            first_line: first_line(&queued.user_text),
            text: for_holder.then(|| queued.user_text.clone()),
        };
        self.flow.queue.iter().map(entry).collect()
    }

    /// The chat's view of an ended authoring or pressure session.
    pub(super) fn ended_chat(&self) -> EndedChat {
        EndedChat {
            session: self.view(),
            turns: self.turns(),
        }
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    line.chars().take(FIRST_LINE_CHARS).collect()
}
