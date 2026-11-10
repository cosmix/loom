//! The quiet period: a background rebuild waits until its summary's sources stop changing.
//!
//! The facade reports every summary whose inputs changed in a watcher batch
//! ([`Scheduler::sources_changed`]); a queued background id starts once [`QUIET_PERIOD`] has
//! passed since its last change, and since the end of the authoring turn it changed in. A press
//! or an approved question skips the wait. An id with no change on record (one out of date when
//! the server started) waits for nothing.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use super::{Scheduler, SessionSpawner, Via, QUIET_PERIOD};

/// The summaries whose sources changed lately.
#[derive(Debug, Default)]
pub(super) struct Quiet {
    /// The start of each id's quiet period: its last change, or the end of the authoring turn
    /// it changed in. An id without an entry has been quiet long enough.
    since: BTreeMap<String, Instant>,
    /// Ids changed while the authoring turn is open; their period restarts when it ends.
    in_turn: BTreeSet<String>,
}

impl Quiet {
    /// Whether `id` is still inside its quiet period at `now`.
    pub(super) fn waits(&self, id: &str, now: Instant) -> bool {
        let since = self.since.get(id);
        since.is_some_and(|since| !ran_out(*since, now))
    }

    /// Keep only the ids `keep` holds.
    pub(super) fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.since.retain(|id, _| keep(id));
        self.in_turn.retain(|id| keep(id));
    }

    /// Drop the periods that ran out by `now`; true when one did.
    fn expire(&mut self, now: Instant) -> bool {
        let before = self.since.len();
        self.since.retain(|_, since| !ran_out(*since, now));
        self.since.len() != before
    }
}

fn ran_out(since: Instant, now: Instant) -> bool {
    now.saturating_duration_since(since) >= QUIET_PERIOD
}

impl Scheduler {
    /// The inputs of `ids` changed at `now` (a source edited, an overview's member resealed):
    /// their background rebuilds wait [`QUIET_PERIOD`] from now, or from the end of the
    /// authoring turn when one is open. Settling a document changes no summary's inputs.
    pub fn sources_changed(&mut self, ids: &[String], now: Instant) {
        for id in ids {
            self.quiet.since.insert(id.clone(), now);
            if self.turn.is_some() {
                self.quiet.in_turn.insert(id.clone());
            }
        }
    }

    /// The queued background ids waiting for edits to stop: inside their quiet period at `now`,
    /// or changed in the authoring turn that is still open. In queue order.
    pub fn waiting(&self, now: Instant) -> Vec<String> {
        let in_turn = |id: &str| self.turn.is_some() && self.quiet.in_turn.contains(id);
        let queued = self.queue.iter().filter(|queued| queued.via == Via::Batch);
        queued
            .filter(|queued| self.quiet.waits(&queued.id, now) || in_turn(&queued.id))
            .map(|queued| queued.id.clone())
            .collect()
    }

    /// The authoring turn ended at `now`: what changed during it waits a full period from here,
    /// since the reader's next turn often edits the same document again.
    pub(super) fn restart_turn_changes(&mut self, now: Instant) {
        for id in std::mem::take(&mut self.quiet.in_turn) {
            self.quiet.since.insert(id, now);
        }
    }

    /// Try a start when a quiet period ran out by `now`.
    pub(super) fn wake_quiet(&mut self, now: Instant, spawner: &dyn SessionSpawner) {
        if self.quiet.expire(now) {
            self.try_start(now, spawner);
        }
    }
}
