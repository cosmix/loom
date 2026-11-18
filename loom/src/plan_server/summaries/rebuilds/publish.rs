//! What the facade tells browsers and the terminal after a scheduler call, always after the
//! state lock is released.

use std::collections::{BTreeMap, BTreeSet};

use super::{lock, Rebuilds};
use crate::plan_server::concept::page_keys;
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::summaries::scheduler::StopReason;
use crate::plan_server::summaries::scheduler::{RebuildReason, Scheduler, SchedulerEvent};
use crate::plan_server::wire::concept::ConceptChanged;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::summaries::SummaryPanel;
use crate::planning::summary::store::Scope;

/// Per id: its out-of-date reason and the running rebuild's `(done, total)` its pages show.
type Marks = BTreeMap<String, (Option<RebuildReason>, Option<(usize, usize)>)>;

/// The scheduler state concept mode and the terminal follow.
pub(super) struct Snapshot {
    marks: Marks,
    start_error: Option<String>,
}

impl Snapshot {
    pub(super) fn of(scheduler: &Scheduler) -> Snapshot {
        let view = scheduler.panel_state();
        let progress = scheduler.progress();
        let running = view.running.map(|(_, ids)| ids).unwrap_or_default();
        let mut ids: BTreeSet<&String> = view.failed.iter().map(|(id, _)| id).collect();
        ids.extend(view.question.iter().flat_map(|open| open.ids.iter()));
        ids.extend(running.iter());
        let mark = |id: &String| {
            let building = progress.filter(|_| running.contains(id));
            (id.clone(), (scheduler.reason(id), building))
        };
        Snapshot {
            marks: ids.into_iter().map(mark).collect(),
            start_error: view.start_error,
        }
    }
}

impl Rebuilds {
    /// Broadcast the question messages, concept mode's `concept_changed` for every id whose
    /// reason or progress changed since `before`, and the panel; print the terminal lines.
    ///
    /// Another call may close or prune the question between this call's scheduler step and
    /// its broadcast. So an asked question is sent as the state holds it now, or not at all
    /// once closed, and everything is sent under `panel_sent`: a call that read the state
    /// earlier also sends earlier, so a stale question never follows its closing message.
    pub(super) fn publish(&self, before: &Snapshot, events: &[SchedulerEvent]) {
        let mut sent = lock(&self.panel_sent);
        let (after, scopes, mut asked) = {
            let state = self.lock();
            let open = state.scheduler.panel_state().question;
            let asked: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    SchedulerEvent::Asked(event) => {
                        let current = open.clone().filter(|open| open.id == event.id);
                        Some(current.map(|open| (state.pending(&open.ids), open)))
                    }
                    _ => None,
                })
                .collect();
            let scopes = state.latest.as_ref().map(|eval| eval.scopes.clone());
            (
                Snapshot::of(&state.scheduler),
                scopes.unwrap_or_default(),
                asked.into_iter(),
            )
        };
        for event in events {
            announce(event);
            match event {
                SchedulerEvent::Asked(_) => {
                    if let Some((pending, open)) = asked.next().flatten() {
                        let question = self.summary_question(&open, &pending);
                        (self.broadcast)(&PlanMessage::SummaryQuestion(question));
                    }
                }
                SchedulerEvent::QuestionClosed { id } => {
                    let id = id.clone();
                    (self.broadcast)(&PlanMessage::SummaryQuestionClosed { id });
                }
                _ => {}
            }
        }
        if after.start_error != before.start_error {
            if let Some(why) = &after.start_error {
                line(&format!("summaries: cannot start a rebuild: {}", safe(why)));
            }
        }
        self.concept_changed(&before.marks, &after.marks, &scopes);
        self.send_panel(&mut sent);
    }

    fn concept_changed(&self, before: &Marks, after: &Marks, scopes: &BTreeMap<String, Scope>) {
        let ids: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
        let changed = ids
            .into_iter()
            .filter(|id| before.get(*id) != after.get(*id));
        let changed: Vec<String> = changed.cloned().collect();
        for scope in scopes.values() {
            // Membership resolves a plan's feature without taking the project lock.
            // Standalone plans belong to no feature scope and emit no concept change.
            let out_of_date: Vec<String> = changed
                .iter()
                .filter(|id| !page_keys(id, scope).is_empty())
                .cloned()
                .collect();
            if out_of_date.is_empty() {
                continue;
            }
            let changed = ConceptChanged {
                feature: Some(scope.feature.clone()),
                pages: pages_of(&out_of_date, scope),
                out_of_date,
                sources: Vec::new(),
            };
            (self.broadcast)(&PlanMessage::ConceptChanged(changed));
        }
    }

    /// Broadcast the panel when it differs from the one last sent.
    pub(super) fn publish_panel(&self) {
        // `panel_sent` is taken before the panel is built and held across the broadcast, so a
        // panel built later is never sent earlier. Nothing that holds the state or log lock
        // takes `panel_sent`, so building the panel under it cannot invert the lock order.
        let mut sent = lock(&self.panel_sent);
        self.send_panel(&mut sent);
    }

    /// Broadcast the panel unless it equals `sent`, the panel last sent (held locked).
    fn send_panel(&self, sent: &mut Option<SummaryPanel>) {
        let panel = self.panel();
        if sent.as_ref() == Some(&panel) {
            return;
        }
        (self.broadcast)(&PlanMessage::SummaryPanel(Box::new(panel.clone())));
        *sent = Some(panel);
    }
}

/// The concept pages showing `ids`, read from the evaluation's scope so no project lock is taken.
fn pages_of(ids: &[String], scope: &Scope) -> Vec<String> {
    let pages: BTreeSet<String> = ids.iter().flat_map(|id| page_keys(id, scope)).collect();
    pages.into_iter().collect()
}

/// The terminal line of one event.
fn announce(event: &SchedulerEvent) {
    let text = match event {
        SchedulerEvent::Started { session, ids } => {
            format!(
                "summaries: rebuilding {} (session {})",
                ids.len(),
                safe(session)
            )
        }
        SchedulerEvent::Asked(open) => format!("summaries: {} out of date, asking", open.ids.len()),
        SchedulerEvent::QuestionClosed { .. } => return,
        SchedulerEvent::Stopped { session, reason } => stopped_line(&safe(session), reason),
        SchedulerEvent::Failed { ids, error } => {
            format!("summaries: {} failed: {}", ids.len(), safe(error))
        }
    };
    line(&text);
}

fn stopped_line(session: &str, reason: &StopReason) -> String {
    match reason {
        StopReason::ClaimsDone => {
            format!("summaries: stopping session {session}: its claims are sealed or released")
        }
        StopReason::Stalled => format!("summaries: stopping session {session}: no seal in time"),
        StopReason::Ended => format!("summaries: session {session} ended"),
        StopReason::Failed(why) => format!("summaries: session {session} failed: {}", safe(why)),
        StopReason::ServerStopping => {
            format!("summaries: stopping; session {session} finishes its task")
        }
    }
}
