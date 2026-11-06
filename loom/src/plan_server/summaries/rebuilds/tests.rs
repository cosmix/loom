//! The facade: starts and stops outside the lock, failed starts, guards, seals read from
//! evaluations, the question's estimate, the start-up seed.

use std::path::PathBuf;
use std::sync::OnceLock;

use super::*;
use crate::plan_server::summaries::scheduler::{AnswerRefused, AnswerRight, SuspendReason};
use crate::plan_session::bridge::{BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::planning::summary::claims::Claim;
use crate::planning::summary::cost::estimate::EstimateBasis;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::engine::EngineUpdate;
use crate::planning::summary::evaluate::{SummaryEval, SummaryState};
use crate::planning::summary::format::{FrontMatter, SummaryKind};
use crate::planning::summary::store::Scope;

use SummaryState::{Current, Missing, Rebuilding};

/// A spawner that records whether the facade's state lock was free at each call.
#[derive(Default)]
pub(super) struct Probe {
    pub(super) rebuilds: OnceLock<Weak<Rebuilds>>,
    starts: Mutex<Vec<Vec<String>>>,
    pub(super) stops: Mutex<Vec<(String, bool)>>,
    free: Mutex<Vec<bool>>,
    fail: Mutex<Option<String>>,
    /// The session id the next start answers instead of `s<n>`.
    next_id: Mutex<Option<String>>,
    /// An end the next start delivers for its own session before it returns.
    end_first: Mutex<Option<(Arc<Bridge>, EndOutcome)>>,
    /// Every start goes to this bridge, as the real spawner's does.
    bridge: OnceLock<Arc<Bridge>>,
}

impl Probe {
    fn note_lock(&self) {
        let rebuilds = self.rebuilds.get().and_then(Weak::upgrade);
        let free = rebuilds.is_some_and(|rebuilds| rebuilds.inner.try_lock().is_ok());
        self.free.lock().unwrap().push(free);
    }

    pub(super) fn starts(&self) -> Vec<Vec<String>> {
        self.starts.lock().unwrap().clone()
    }

    /// Answer the next start with `id`, a session the test's bridge knows.
    pub(super) fn answer_next_start(&self, id: &str) {
        *self.next_id.lock().unwrap() = Some(id.to_owned());
    }

    /// The next start's session ends with `outcome`, delivered through `bridge`'s event before
    /// the start returns, as a launch that fails at once does.
    pub(super) fn end_next_start_first(&self, bridge: &Arc<Bridge>, outcome: EndOutcome) {
        *self.end_first.lock().unwrap() = Some((Arc::clone(bridge), outcome));
    }

    /// Start every session in `bridge` and answer the bridge's session id.
    pub(super) fn start_in(&self, bridge: &Arc<Bridge>) {
        self.bridge.set(Arc::clone(bridge)).ok();
    }
}

impl SessionSpawner for Probe {
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        self.note_lock();
        if let Some(why) = self.fail.lock().unwrap().clone() {
            anyhow::bail!(why);
        }
        let own = {
            let mut starts = self.starts.lock().unwrap();
            starts.push(ids.to_vec());
            format!("s{}", starts.len())
        };
        if let Some(bridge) = self.bridge.get() {
            let request = super::tests_alive::start_request(SessionKind::Summary);
            return bridge
                .start_session(request)
                .map_err(|error| anyhow::anyhow!(error));
        }
        let session = self.next_id.lock().unwrap().take().unwrap_or(own);
        let end_first = self.end_first.lock().unwrap().take();
        let rebuilds = self.rebuilds.get().and_then(Weak::upgrade);
        if let (Some((bridge, outcome)), Some(rebuilds)) = (end_first, rebuilds) {
            let (id, kind) = (session.clone(), SessionKind::Summary);
            rebuilds.on_bridge_event(&bridge, &BridgeEvent::Ended { id, kind, outcome });
        }
        Ok(session)
    }

    fn stop(&self, session: &str, release_claims: bool) {
        self.note_lock();
        let stop = (session.to_owned(), release_claims);
        self.stops.lock().unwrap().push(stop);
    }
}

pub(super) type Sent = Arc<Mutex<Vec<PlanMessage>>>;

pub(super) fn facade() -> (Arc<Probe>, Arc<Rebuilds>, Sent) {
    let probe = Arc::new(Probe::default());
    let sent: Sent = Arc::default();
    let sink = Arc::clone(&sent);
    let broadcast =
        Arc::new(move |message: &PlanMessage| sink.lock().unwrap().push(message.clone()));
    let rebuilds = Rebuilds::new_for_test(probe.clone(), None, broadcast);
    probe.rebuilds.set(Arc::downgrade(&rebuilds)).ok();
    (probe, rebuilds, sent)
}

pub(super) fn file(title: &str) -> SummaryFile {
    let front = FrontMatter {
        kind: SummaryKind::Document,
        of: "doc/plans/demo/PLAN-x.md".into(),
        file: None,
        bundle: BTreeMap::new(),
        cites: BTreeMap::new(),
        members: BTreeMap::new(),
    };
    let (title, blocks) = (title.to_owned(), Vec::new());
    SummaryFile {
        front,
        title,
        blocks,
    }
}

/// `(id, state, title of its file, claimant)`.
pub(super) type Row<'a> = (&'a str, SummaryState, Option<&'a str>, Option<&'a str>);

fn summary(&(id, state, title, session): &Row<'_>) -> SummaryEval {
    let claim = session.map(|session| Claim {
        summary: id.to_owned(),
        session: Some(session.to_owned()),
        pid: 1,
        created_at: String::new(),
    });
    SummaryEval {
        id: id.to_owned(),
        source: if let Some(slug) = id.strip_prefix("plans/") {
            Some(PathBuf::from(format!("doc/plans/demo/PLAN-{slug}.md")))
        } else {
            id.strip_prefix("demo/product/")
                .map(|section| PathBuf::from(format!("doc/product/demo/{section}.md")))
        },
        state,
        file: title.map(file),
        sentences: Vec::new(),
        claim,
        error: None,
    }
}

pub(super) fn update(rows: &[Row<'_>], invalidated: &[&str]) -> EngineUpdate {
    let summaries = rows.iter().map(|row| (row.0.to_owned(), summary(row)));
    let ids = rows.iter().map(|row| row.0);
    let scope = Scope {
        feature: "demo".into(),
        product: ids
            .clone()
            .filter(|id| id.starts_with("demo/product/"))
            .map(str::to_owned)
            .collect(),
        engineering: ids
            .filter(|id| id.starts_with("plans/"))
            .map(str::to_owned)
            .collect(),
        outside: Vec::new(),
    };
    let evaluation = Evaluation {
        summaries: summaries.collect(),
        scopes: BTreeMap::from([("demo".into(), scope)]),
        orphans: Vec::new(),
    };
    EngineUpdate {
        evaluation,
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated: invalidated.iter().map(|id| id.to_string()).collect(),
    }
}

pub(super) fn owned(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn starts_and_stops_run_outside_the_state_lock() {
    let (probe, rebuilds, _) = facade();
    let missing = [
        ("plans/a", Missing, None, None),
        ("plans/b", Missing, None, None),
    ];
    rebuilds.on_update(&update(&missing, &["plans/a", "plans/b"]));
    assert_eq!(probe.starts(), vec![owned(&["plans/a", "plans/b"])]);
    assert_eq!(rebuilds.panel().running.expect("s1 runs").session, "s1");
    let a_sealed = [
        ("plans/a", Current, Some("A"), None),
        ("plans/b", Missing, None, Some("s1")),
    ];
    rebuilds.on_update(&update(&a_sealed, &[]));
    assert_eq!(rebuilds.building(), Some(Building { done: 1, total: 2 }));
    assert!(probe.stops.lock().unwrap().is_empty(), "b is still claimed");
    let both = [
        ("plans/a", Current, Some("A"), None),
        ("plans/b", Current, Some("B"), None),
    ];
    rebuilds.on_update(&update(&both, &[]));
    assert_eq!(*probe.stops.lock().unwrap(), vec![("s1".to_owned(), false)]);
    assert!(
        probe.free.lock().unwrap().iter().all(|free| *free),
        "never under the lock"
    );
}

#[test]
fn a_failed_start_shows_in_the_panel_and_keeps_the_ids() {
    let (probe, rebuilds, sent) = facade();
    *probe.fail.lock().unwrap() = Some("start blocked: a pressure run".into());
    let ids = owned(&["plans/a", "plans/b"]);
    rebuilds.enqueue(ids.clone(), Trigger::Button);
    let panel = rebuilds.panel();
    assert_eq!(
        panel.start_error.as_deref(),
        Some("start blocked: a pressure run")
    );
    assert_eq!((panel.queue, panel.running), (ids.clone(), None));
    let concept = |message: &PlanMessage| matches!(message, PlanMessage::ConceptChanged(_));
    assert!(
        !sent.lock().unwrap().iter().any(concept),
        "nothing is building"
    );
    *probe.fail.lock().unwrap() = None;
    rebuilds.enqueue(ids.clone(), Trigger::Button);
    let panel = rebuilds.panel();
    assert_eq!(
        (panel.start_error, panel.running.map(|r| r.ids)),
        (None, Some(ids))
    );
}

#[test]
fn a_suspend_guard_holds_rebuilds_until_it_drops() {
    let (probe, rebuilds, _) = facade();
    let guard = rebuilds.suspend(SuspendReason::PressureRun);
    let missing = [("plans/a", Missing, None, None)];
    rebuilds.on_update(&update(&missing, &["plans/a"]));
    assert!(probe.starts().is_empty(), "a pressure run holds");
    drop(guard);
    assert_eq!(probe.starts(), vec![owned(&["plans/a"])]);
}

#[test]
fn the_question_carries_an_estimate_and_checks_who_answers() {
    let (probe, rebuilds, sent) = facade();
    let ids: Vec<String> = (1..=6).map(|i| format!("plans/p{i}")).collect();
    let rows: Vec<Row<'_>> = ids
        .iter()
        .map(|id| (id.as_str(), Missing, None, None))
        .collect();
    let invalidated: Vec<&str> = ids.iter().map(String::as_str).collect();
    rebuilds.on_update(&update(&rows, &invalidated));
    let question = rebuilds.panel().question.expect("six ask");
    let starting = (question.count, question.estimate_tokens, question.basis);
    assert_eq!(starting, (6, 240_000, EstimateBasis::Starting));
    let asked = |message: &PlanMessage| match message {
        PlanMessage::SummaryQuestion(sent) => Some(sent.clone()),
        _ => None,
    };
    let broadcast = sent.lock().unwrap().iter().find_map(asked);
    assert_eq!(broadcast, Some(question.clone()));
    let reason = rebuilds.out_of_date_reason(&ids[0]);
    assert_eq!(reason, Some(RebuildReason::AwaitingApproval));
    let estimate = rebuilds.estimate_text();
    assert_eq!(
        estimate.as_deref(),
        Some("about 240000 tokens to build, an estimate")
    );
    let refused = rebuilds.answer(&question.id, true, AnswerRight::NotAllowed);
    assert_eq!(refused, Err(AnswerRefused::NotAllowed));
    assert!(probe.starts().is_empty());
}

#[test]
fn recovery_queues_what_dead_sessions_held() {
    let (probe, rebuilds, _) = facade();
    let rows = [
        ("plans/dead", Rebuilding, Some("old"), Some("dead1")),
        ("plans/live", Rebuilding, Some("old"), Some("live1")),
        ("plans/stale", Missing, None, None),
        ("plans/done", Current, Some("done"), None),
    ];
    let latest = Arc::new(update(&rows, &[]).evaluation);
    rebuilds.after_recovery(&owned(&["live1"]), &[], latest);
    assert_eq!(probe.starts(), vec![owned(&["plans/dead", "plans/stale"])]);
}
