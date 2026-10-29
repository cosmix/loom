//! The queue and the start decision: pruning, re-queueing, the limit, the question.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use super::{QuestionView, Queued, Running, Scheduler, SchedulerEvent, SessionSpawner, Via};

impl Scheduler {
    /// Drop every queued, declined, failed, unsealed and asked id that is no longer out of date.
    pub(super) fn prune(&mut self, out_of_date: &[String]) {
        let keep: BTreeSet<&str> = out_of_date.iter().map(String::as_str).collect();
        self.queue
            .retain(|queued| keep.contains(queued.id.as_str()));
        self.declined.retain(|id| keep.contains(id.as_str()));
        self.failed.retain(|id, _| keep.contains(id.as_str()));
        self.unsealed.retain(|id| keep.contains(id.as_str()));
        let Some(open) = self.question.as_mut() else {
            return;
        };
        let asked = open.ids.len();
        open.ids.retain(|id| keep.contains(id.as_str()));
        if open.ids.is_empty() {
            let id = open.id.clone();
            self.question = None;
            self.out.push(SchedulerEvent::QuestionClosed { id });
        } else if open.ids.len() != asked {
            self.out.push(SchedulerEvent::Asked(open.clone()));
        }
    }

    /// Prune, then queue every out-of-date id that is not declined, failed, pending in the
    /// session or queued (a turn's end, a resume).
    pub(super) fn refresh(&mut self, out_of_date: &[String]) {
        self.prune(out_of_date);
        for id in out_of_date {
            let known = self.declined.contains(id) || self.failed.contains_key(id);
            if !known && !self.in_session(id) && !self.is_queued(id) {
                self.push_batch(id);
            }
        }
    }

    /// A Button pressed for `ids`: forget that they were declined, failed or left unsealed.
    pub(super) fn forget(&mut self, ids: &[String]) {
        for id in ids {
            self.declined.remove(id);
            self.failed.remove(id);
            self.unsealed.remove(id);
        }
    }

    /// Queue `id` at the back; nothing is queued once the server stops.
    pub(super) fn push_batch(&mut self, id: &str) {
        if self.stopping {
            return;
        }
        let (id, via) = (id.to_owned(), Via::Batch);
        self.queue.push(Queued { id, via });
    }

    /// The running session still has `id` to seal.
    pub(super) fn in_session(&self, id: &str) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| running.pending(id))
    }

    pub(super) fn is_queued(&self, id: &str) -> bool {
        self.queue.iter().any(|queued| queued.id == id)
    }

    /// Move `ids` to the end of the queue's front part (Button and approved ids, in arrival
    /// order) marked `via`; a Button mark is kept. Ids the session still has are skipped, and
    /// nothing is queued once the server stops.
    pub(super) fn promote(&mut self, ids: &[String], via: Via) {
        if self.stopping {
            return;
        }
        let mut buttons = BTreeSet::new();
        self.queue.retain(|queued| {
            let moved = ids.contains(&queued.id);
            if moved && queued.via == Via::Button {
                buttons.insert(queued.id.clone());
            }
            !moved
        });
        let mut at = self.batch_start();
        for id in ids {
            if self.in_session(id) || self.is_queued(id) {
                continue;
            }
            let via = if buttons.contains(id) {
                Via::Button
            } else {
                via
            };
            self.queue.insert(
                at,
                Queued {
                    id: id.clone(),
                    via,
                },
            );
            at += 1;
        }
    }

    /// Put a session's unsealed ids back at the front of their part of the queue.
    pub(super) fn requeue(&mut self, ids: Vec<String>, vias: &BTreeMap<String, Via>) {
        if self.stopping {
            return;
        }
        let ids: Vec<String> = ids.into_iter().filter(|id| !self.is_queued(id)).collect();
        let via = |id: &String| vias.get(id).copied().unwrap_or(Via::Batch);
        let (front, batch): (Vec<String>, Vec<String>) =
            ids.into_iter().partition(|id| via(id) != Via::Batch);
        for (at, id) in front.into_iter().enumerate() {
            let via = via(&id);
            self.queue.insert(at, Queued { id, via });
        }
        let start = self.batch_start();
        for (at, id) in batch.into_iter().enumerate() {
            self.queue.insert(
                start + at,
                Queued {
                    id,
                    via: Via::Batch,
                },
            );
        }
    }

    fn batch_start(&self) -> usize {
        let batch = self
            .queue
            .iter()
            .position(|queued| queued.via == Via::Batch);
        batch.unwrap_or(self.queue.len())
    }

    /// Start the queue when nothing blocks it: all of it within the limit or when every id
    /// skips the limit, else the Button and approved ids alone, else ask about the queue.
    pub(super) fn try_start(&mut self, now: Instant, spawner: &dyn SessionSpawner) {
        let blocked = self.config.read_only.is_some()
            || self.stopping
            || !self.suspensions.is_empty()
            || self.running.is_some()
            || self.question.is_some();
        if blocked || self.queue.is_empty() {
            return;
        }
        let all: Vec<String> = self.queue.iter().map(|queued| queued.id.clone()).collect();
        let front: Vec<String> = self.queue[..self.batch_start()]
            .iter()
            .map(|queued| queued.id.clone())
            .collect();
        let ids = if all.len() <= self.config.limit || front.len() == all.len() {
            all
        } else if !front.is_empty() {
            front
        } else {
            self.asked += 1;
            let question = QuestionView {
                id: format!("q{}", self.asked),
                ids: all,
            };
            self.question = Some(question.clone());
            self.out.push(SchedulerEvent::Asked(question));
            return;
        };
        self.start(ids, now, spawner);
    }

    /// Start one session for `ids`; on an error they stay queued and the panel shows why.
    fn start(&mut self, ids: Vec<String>, now: Instant, spawner: &dyn SessionSpawner) {
        let session = match spawner.start_summary(&ids) {
            Ok(session) => session,
            Err(error) => {
                self.start_error = Some(format!("{error:#}"));
                return;
            }
        };
        let mut vias = BTreeMap::new();
        self.queue.retain(|queued| {
            let started = ids.contains(&queued.id);
            if started {
                vias.insert(queued.id.clone(), queued.via);
            }
            !started
        });
        self.start_error = None;
        self.running = Some(Running {
            session: session.clone(),
            ids: ids.clone(),
            vias,
            since: now,
            sealed: BTreeSet::new(),
            claimed: BTreeSet::new(),
            stopping: false,
            seal_error: None,
        });
        self.out.push(SchedulerEvent::Started { session, ids });
    }
}
