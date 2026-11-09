//! Calls that race: a session's end arriving before its start settles, a question closed or
//! pruned before an earlier call published it; and what concept mode and the stall timer see
//! of stalls and seals.

use std::thread;
use std::time::Duration;

use std::net::SocketAddr;

use super::publish::Snapshot;
use super::tests::{facade, owned, update, Probe, Row, Sent};
use super::tests_bridge::{state, turn_ended, Rig, ID};
use super::*;
use crate::plan_server::context::BindInfo;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::summaries::scheduler::{AnswerRight, QuestionView, NO_SEAL_TIMEOUT};
use crate::plan_server::wire::concept::ConceptChanged;
use crate::plan_server::wire::summaries::{FailedSummary, SummaryQuestion};
use crate::plan_session::bridge::{BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::SessionState;
use crate::plan_session::tmux_host::TmuxHost;
use crate::planning::runtime::create_runtime_dir;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::engine::EngineUpdate;
use crate::planning::summary::evaluate::fixtures::{Fixture, ID1, PRODUCT, S1, VISION, VISION_ID};
use crate::planning::summary::evaluate::SummaryState::{Current, Missing, Rebuilding, Stale};
use crate::planning::summary::evaluate::{evaluate, fixtures};
use crate::planning::summary::store::Scope;
use crate::planning::watch::{Batch, OwnWrites};

/// Regression: a launch that fails at once delivers its session's end before `start_summary`
/// returns, while the scheduler still runs the pending placeholder. The end must fail the ids
/// with its reason, not be dropped and leave a dead session running until the tick's generic
/// "the session failed".
#[test]
fn an_end_before_the_start_settles_fails_the_ids_with_its_reason() {
    let rig = Rig::new(true);
    let why = "the launch was refused: the brief was not delivered";
    let failed = EndOutcome::Failed(why.into());
    rig.probe.end_next_start_first(&rig.bridge, failed);
    rig.report_missing();
    let failed = FailedSummary {
        id: ID.to_owned(),
        error: why.to_owned(),
    };
    let panel = rig.rebuilds.panel();
    assert_eq!((panel.failed, panel.running), (vec![failed.clone()], None));
    rig.rebuilds.tick();
    rig.rebuilds.tick();
    assert_eq!(
        rig.rebuilds.panel().failed,
        vec![failed],
        "the ticks change nothing"
    );
    assert_eq!(rig.starts().len(), 1, "failed ids are not restarted");
}

/// The same race with a normal end: the ids return to the queue and start again.
#[test]
fn an_end_before_the_start_settles_requeues_and_restarts_the_ids() {
    let rig = Rig::new(false);
    rig.probe
        .end_next_start_first(&rig.bridge, EndOutcome::Closed);
    rig.report_missing();
    assert_eq!(rig.starts(), vec![owned(&[ID]), owned(&[ID])], "s1 ended");
    assert_eq!(rig.rebuilds.panel().running.expect("s2 runs").session, "s2");
}

fn questions(sent: &Sent) -> Vec<SummaryQuestion> {
    let sent = sent.lock().unwrap();
    let question = |message: &PlanMessage| match message {
        PlanMessage::SummaryQuestion(question) => Some(question.clone()),
        _ => None,
    };
    sent.iter().filter_map(question).collect()
}

/// Regression: a question event computed under an earlier lock is published as the state
/// holds the question when sent: pruned ids are left out and a closed question is not sent,
/// so a late broadcast never shows browsers a question that no longer exists.
#[test]
fn a_question_is_published_as_the_state_holds_it_when_sent() {
    let (_, rebuilds, sent) = facade();
    let ids: Vec<String> = (1..=7).map(|i| format!("plans/p{i}")).collect();
    let rows: Vec<Row<'_>> = ids
        .iter()
        .map(|id| (id.as_str(), Missing, None, None))
        .collect();
    let invalidated: Vec<&str> = ids.iter().map(String::as_str).collect();
    rebuilds.on_update(&update(&rows, &invalidated));
    let asked = rebuilds.panel().question.expect("seven ask");
    let stale = SchedulerEvent::Asked(QuestionView {
        id: asked.id.clone(),
        ids: asked.ids,
    });
    rebuilds.on_update(&update(&rows[1..], &[]));
    sent.lock().unwrap().clear();
    let before = Snapshot::of(&rebuilds.lock().scheduler);
    rebuilds.publish(&before, std::slice::from_ref(&stale));
    let published: Vec<Vec<String>> = questions(&sent).into_iter().map(|q| q.ids).collect();
    assert_eq!(published, vec![ids[1..].to_vec()], "the pruned question");
    let declined = rebuilds.answer(&asked.id, false, AnswerRight::Holder);
    assert_eq!(declined, Ok(()));
    sent.lock().unwrap().clear();
    let before = Snapshot::of(&rebuilds.lock().scheduler);
    rebuilds.publish(&before, &[stale]);
    assert_eq!(
        questions(&sent),
        vec![],
        "a closed question is not sent again"
    );
}

fn concept_changes(sent: &Sent) -> Vec<ConceptChanged> {
    let sent = sent.lock().unwrap();
    let changed = |message: &PlanMessage| match message {
        PlanMessage::ConceptChanged(changed) => Some(changed.clone()),
        _ => None,
    };
    sent.iter().filter_map(changed).collect()
}

/// The `concept_changed` naming `ids` and the pages that show them.
fn changed(ids: &[&str]) -> ConceptChanged {
    let mut pages: Vec<String> = ids
        .iter()
        .map(|id| format!("demo/engineering/{}", id.strip_prefix("plans/").unwrap()))
        .collect();
    pages.push("overview/demo/engineering".into());
    ConceptChanged {
        feature: Some("demo".into()),
        pages,
        out_of_date: owned(ids),
        sources: Vec::new(),
    }
}

#[test]
fn a_stall_failing_the_running_ids_notifies_concept_mode() {
    // A zero timeout makes the first tick a stall; the production timeout is ten minutes.
    let config = SchedulerConfig {
        no_seal_timeout: Duration::ZERO,
        ..SchedulerConfig::standard(None)
    };
    let rig = Rig::build(Arc::new(TmuxHost), false, config, false);
    rig.report_missing();
    rig.sent.lock().unwrap().clear();
    rig.rebuilds.tick();
    assert_eq!(rig.stops(), vec![("s1".to_owned(), true)], "stalled");
    let running = rig.rebuilds.panel().running.map(|running| running.session);
    assert_eq!(running.as_deref(), Some("s1"), "s1 runs until its end");
    assert_eq!(concept_changes(&rig.sent), vec![changed(&[ID])]);
}

#[test]
fn a_seal_changing_the_done_count_notifies_concept_mode() {
    let (_, rebuilds, sent) = facade();
    let missing = [
        ("plans/a", Missing, None, None),
        ("plans/b", Missing, None, None),
    ];
    rebuilds.on_update(&update(&missing, &["plans/a", "plans/b"]));
    sent.lock().unwrap().clear();
    let a_sealed = [
        ("plans/a", Current, Some("A"), None),
        ("plans/b", Missing, None, Some("s1")),
    ];
    rebuilds.on_update(&update(&a_sealed, &[]));
    assert_eq!(rebuilds.building(), Some(Building { done: 1, total: 2 }));
    let expected = changed(&["plans/a", "plans/b"]);
    assert_eq!(concept_changes(&sent), vec![expected]);
}

#[test]
fn concept_changes_are_grouped_by_feature_and_exclude_standalone_plans() {
    let fixture = fixtures::two_features();
    let project = fixtures::load(&fixture.root);
    let (_, rebuilds, sent) = facade();
    let update = EngineUpdate {
        evaluation: evaluate(&project, &[]),
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated: Vec::new(),
    };
    rebuilds.on_update(&update);
    sent.lock().unwrap().clear();

    rebuilds.enqueue(
        owned(&[
            "demo/product/section_01",
            "plans/x",
            "plans/y",
            "plans/solo",
        ]),
        Trigger::Button,
    );

    assert_eq!(
        concept_changes(&sent),
        vec![
            ConceptChanged {
                feature: Some("demo".into()),
                pages: owned(&[
                    "demo/engineering/x",
                    "demo/product/section_01",
                    "overview/demo/engineering",
                    "overview/demo/product",
                ]),
                out_of_date: owned(&["demo/product/section_01", "plans/x"]),
                sources: Vec::new(),
            },
            ConceptChanged {
                feature: Some("other".into()),
                pages: owned(&["other/engineering/y", "overview/other/engineering"]),
                out_of_date: owned(&["plans/y"]),
                sources: Vec::new(),
            },
        ]
    );
}

#[test]
fn an_overview_change_notifies_only_its_own_feature() {
    let (_, rebuilds, sent) = facade();
    let rows = [("overview/demo/product", Missing, None, None)];
    let mut update = update(&rows, &["overview/demo/product"]);
    let other = Scope {
        feature: "other".into(),
        ..Scope::default()
    };
    update.evaluation.scopes.insert("other".into(), other);
    rebuilds.on_update(&update);

    assert_eq!(
        concept_changes(&sent),
        vec![ConceptChanged {
            feature: Some("demo".into()),
            pages: owned(&["overview/demo/product"]),
            out_of_date: owned(&["overview/demo/product"]),
            sources: Vec::new(),
        }]
    );
}

/// A seal is a changed file of a running id that evaluates non-missing: a file that vanished
/// or fails validation (Missing) neither counts as sealed nor restarts the stall timer.
#[test]
fn a_running_summary_evaluating_missing_is_not_sealed_and_keeps_the_stall_timer() {
    let (_, rebuilds, _) = facade();
    let stale: [Row<'_>; 1] = [(ID, Stale, Some("old"), None)];
    rebuilds.on_update(&update(&stale, &[ID]));
    let started = Instant::now();
    thread::sleep(Duration::from_millis(20));
    let gone: [Row<'_>; 1] = [(ID, Missing, None, Some("s1"))];
    rebuilds.on_update(&update(&gone, &[]));
    let invalid: [Row<'_>; 1] = [(ID, Missing, Some("draft"), Some("s1"))];
    rebuilds.on_update(&update(&invalid, &[]));
    let due = started + NO_SEAL_TIMEOUT;
    let unsealed = Some(Building { done: 0, total: 1 });
    assert_eq!(rebuilds.building(), unsealed, "not sealed");
    let stall = rebuilds.lock().scheduler.stall_due(due).map(str::to_owned);
    assert_eq!(
        stall.as_deref(),
        Some("s1"),
        "the timer runs from the start"
    );
    let sealed: [Row<'_>; 1] = [(ID, Rebuilding, Some("new"), Some("s1"))];
    rebuilds.on_update(&update(&sealed, &[]));
    assert_eq!(rebuilds.building(), Some(Building { done: 1, total: 1 }));
    let stall = rebuilds.lock().scheduler.stall_due(due).map(str::to_owned);
    assert_eq!(stall, None, "a seal restarts the timer");
}

/// Seal the vision, section 1 (as one sentence, `text`) and the product overview citing it,
/// each hashed from the sources on disk now.
fn seal_feature(f: &Fixture, text: &str) {
    let vision = "The vision holds.";
    fixtures::seal_document(&f.root, VISION_ID, &[(vision, "specification-sections")]);
    fixtures::seal_document(&f.root, ID1, &[(text, "a")]);
    fixtures::seal_overview(&f.root, PRODUCT, &[(ID1, text)], &[]);
}

/// A facade over a server context on `f`, whose project the test refreshes as the watcher does.
fn served(f: &Fixture) -> (Arc<PlanContext>, Arc<Probe>, Arc<Rebuilds>) {
    create_runtime_dir(&f.paths).expect("runtime dir");
    let bind = BindInfo::new(SocketAddr::from(([127, 0, 0, 1], 0)), "t");
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(f.root.clone(), read_only, bind, OwnWrites::default());
    let ctx = Arc::new(ctx.expect("context").with_runtime(f.paths.clone()));
    let probe = Arc::new(Probe::default());
    let rebuilds = Rebuilds::with_spawner(&ctx, probe.clone());
    probe.rebuilds.set(Arc::downgrade(&rebuilds)).ok();
    (ctx, probe, rebuilds)
}

/// Regression: the chat edits a section inside its turn, the watcher's batch invalidates the
/// summary, and the turn's refinement subagents rebuild and seal it. The turn's end (or the chat
/// going idle) arrives before the batch that shows the seal; the summary is current, so no
/// background session rebuilds it a second time.
#[test]
fn a_seal_made_inside_the_authoring_turn_is_seen_when_the_turn_ends() {
    let ends: [BridgeEvent; 2] = [turn_ended("chat", "t1"), state("chat", SessionState::Idle)];
    for end in ends {
        let rig = Rig::new(false);
        let f = fixtures::fixture(&[
            (VISION, &fixtures::vision(&[1])),
            (S1, &fixtures::section(1, "Alpha.", "Beta.")),
        ]);
        seal_feature(&f, "Alpha holds.");
        let (ctx, probe, rebuilds) = served(&f);
        rebuilds.turn_started(Some(SessionKind::Authoring), "chat", "t1");
        fixtures::put(&f.root, S1, &fixtures::section(1, "Edited.", "Beta."));
        let batch = Batch {
            changed: vec![S1.into()],
            ..Batch::default()
        };
        ctx.project_mut().refresh(&batch).expect("refresh");
        let edited = evaluate(&ctx.project(), &[]);
        let invalidated = evaluation::out_of_date(&edited);
        assert_eq!(invalidated, owned(&[ID1, PRODUCT]), "the edit");
        rebuilds.on_update(&EngineUpdate {
            evaluation: edited,
            rewritten: Vec::new(),
            deleted: Vec::new(),
            invalidated,
        });
        assert_eq!(rebuilds.panel().queue, owned(&[ID1, PRODUCT]), "held");
        seal_feature(&f, "Edited holds.");
        let sealed = evaluate(&ctx.project(), &[]);
        assert_eq!(evaluation::out_of_date(&sealed), Vec::<String>::new());
        rebuilds.on_bridge_event(&rig.bridge, &end);
        assert_eq!(probe.starts(), Vec::<Vec<String>>::new(), "{end:?}");
        assert_eq!(rebuilds.panel().queue, Vec::<String>::new(), "{end:?}");
    }
}
