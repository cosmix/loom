//! What the background frame is built from, and the last frame broadcast.

use std::sync::{Mutex, MutexGuard, PoisonError};

use super::frame;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::bridge::background::BackgroundTask;
use crate::tokens::AgentFlow;

/// Where a frame goes: the hub's broadcast. It never blocks (a full queue drops the tab), so
/// the board holds its lock across it and frames leave in the order they were built.
pub(super) type Emit<'a> = &'a dyn Fn(&PlanMessage);

type Passes = Vec<(String, Vec<AgentFlow>)>;

#[derive(Default)]
pub(super) struct Board {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The chat session whose tasks `tasks` holds.
    session: Option<String>,
    tasks: Vec<BackgroundTask>,
    /// The rows of the last token pass, per group.
    passes: Passes,
    /// The last frame broadcast: what a new tab gets.
    last: Option<PlanMessage>,
}

impl Board {
    /// The bridge's table of `session`, the chat session now. A previous session's frame is
    /// emptied first.
    pub(super) fn tasks(&self, session: &str, tasks: Vec<BackgroundTask>, emit: Emit<'_>) {
        let mut inner = self.lock();
        let replaced = inner.session.take_if(|stored| stored != session);
        if let Some(old) = replaced {
            inner.last = None;
            emit(&frame(&old, &[], &[]));
        }
        inner.session = Some(session.to_owned());
        inner.tasks = tasks;
        inner.publish(emit);
    }

    /// `session` ended: if it is the stored one, its frame is emptied and forgotten.
    pub(super) fn ended(&self, session: &str, emit: Emit<'_>) {
        let mut inner = self.lock();
        if inner.session.take_if(|stored| stored == session).is_some() {
            inner.tasks.clear();
            inner.last = None;
            emit(&frame(session, &[], &[]));
        }
    }

    /// The rows of a token pass. A frame leaves only when joining them changed it: an active
    /// agent's figures move every pass, a settled table does not.
    pub(super) fn flows(&self, passes: Passes, emit: Emit<'_>) {
        let mut inner = self.lock();
        inner.passes = passes;
        inner.publish(emit);
    }

    pub(super) fn replay(&self) -> Option<PlanMessage> {
        self.lock().last.clone()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Every mutation completes before `emit` can panic, so a poisoned lock is usable.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Inner {
    /// Broadcast the joined frame unless it equals the last one (or there is nothing to say yet).
    fn publish(&mut self, emit: Emit<'_>) {
        let Some(session) = &self.session else {
            return;
        };
        let flows = self
            .passes
            .iter()
            .find(|(group, _)| group == session)
            .map_or(&[][..], |(_, flows)| flows.as_slice());
        let next = frame(session, &self.tasks, flows);
        let unchanged = match &self.last {
            Some(last) => *last == next,
            None => self.tasks.is_empty(),
        };
        if unchanged {
            return;
        }
        emit(&next);
        self.last = Some(next);
    }
}
