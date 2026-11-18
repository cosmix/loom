//! The facade over a served project: an edit waits in the panel while a press starts at once, a
//! settle queues nothing, and an uncited edit lists the summary as incomplete until a press.

use std::fs;

use super::super::tests::owned;
use super::super::tests_ordering::{seal_feature, served};
use super::super::Rebuilds;
use super::to_rebuild;
use crate::plan_server::context::PlanContext;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::engine::EngineUpdate;
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::{self, Fixture, ID1, PRODUCT, S1, VISION};
use crate::planning::trace::settle::{mark, SettledLine};
use crate::planning::watch::Batch;

/// The vision, section 01 and the product overview, each summary sealed current.
fn sealed() -> Fixture {
    let f = fixtures::fixture(&[
        (VISION, &fixtures::vision(&[1])),
        (S1, &fixtures::section(1, "Alpha.", "Beta.")),
    ]);
    seal_feature(&f, "Alpha holds.");
    f
}

/// Refresh the served project with a batch naming section 01, as the watcher does, and hand the
/// facade the evaluation with every summary to rebuild as invalidated.
fn batch(ctx: &PlanContext, rebuilds: &Rebuilds) {
    let batch = Batch {
        changed: vec![S1.into()],
        ..Batch::default()
    };
    ctx.project_mut().refresh(&batch).expect("refresh");
    let evaluation = evaluate(&ctx.project(), &[]);
    let invalidated = to_rebuild(&evaluation);
    rebuilds.on_update(&EngineUpdate {
        evaluation,
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated,
    });
}

#[test]
fn an_edit_waits_for_its_quiet_period_and_a_press_starts_at_once() {
    let f = sealed();
    let (ctx, probe, rebuilds) = served(&f);
    batch(&ctx, &rebuilds);
    fixtures::put(&f.root, S1, &fixtures::section(1, "Edited.", "Beta."));
    batch(&ctx, &rebuilds);
    let panel = rebuilds.panel();
    assert_eq!(
        probe.starts(),
        Vec::<Vec<String>>::new(),
        "no rebuild per edit"
    );
    assert_eq!(panel.queue, owned(&[ID1, PRODUCT]));
    assert_eq!(
        panel.waiting,
        owned(&[ID1, PRODUCT]),
        "waiting for edits to stop"
    );
    rebuilds.enqueue(owned(&[ID1]), Trigger::Button);
    assert_eq!(probe.starts(), vec![owned(&[ID1])]);
}

#[test]
fn settling_a_document_whose_summary_is_current_queues_nothing() {
    let f = sealed();
    let (ctx, probe, rebuilds) = served(&f);
    batch(&ctx, &rebuilds);
    let text = fs::read_to_string(f.root.join(S1)).expect("section");
    let line = SettledLine {
        body: "0123456789ab".into(),
        ..SettledLine::default()
    };
    fixtures::put(&f.root, S1, &mark(&text, &line));
    batch(&ctx, &rebuilds);
    let panel = rebuilds.panel();
    assert_eq!(probe.starts(), Vec::<Vec<String>>::new());
    assert_eq!(panel.queue, Vec::<String>::new());
    assert_eq!(panel.incomplete, Vec::<String>::new());
}

#[test]
fn an_uncited_edit_lists_the_summary_as_incomplete_until_a_press() {
    let f = sealed();
    let (ctx, probe, rebuilds) = served(&f);
    batch(&ctx, &rebuilds);
    fixtures::put(&f.root, S1, &fixtures::section(1, "Alpha.", "Beta edited."));
    batch(&ctx, &rebuilds);
    let panel = rebuilds.panel();
    assert_eq!(probe.starts(), Vec::<Vec<String>>::new());
    assert_eq!(
        panel.queue,
        Vec::<String>::new(),
        "not rebuilt in the background"
    );
    assert_eq!(panel.incomplete, owned(&[ID1]), "still shown out of date");
    rebuilds.enqueue(owned(&[ID1]), Trigger::Button);
    assert_eq!(probe.starts(), vec![owned(&[ID1])]);
    assert_eq!(rebuilds.panel().incomplete, Vec::<String>::new(), "running");
}
