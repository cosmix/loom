//! A standalone plan's `has_summary` row field: false until its summary is sealed, then set by
//! the checklist the batch that carried the seal broadcasts.

use super::engine_tests::installed;
use super::fixtures::{self, Fixture};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::watch::apply_batch;
use crate::planning::summary::store;
use crate::planning::watch::Batch;

const SOLO: &str = "doc/plans/PLAN-solo.md";

fn solo() -> Fixture {
    fixtures::fixture(&[(SOLO, "# Plan: Solo\n\nDoes solo.\n\n## Detail\n\nMore.\n")])
}

fn has_summary(ctx: &PlanContext) -> bool {
    ctx.checklist_of(&ctx.project()).standalone[0].has_summary
}

#[test]
fn a_standalone_row_gains_has_summary_in_the_checklist_that_follows_the_seal() {
    let fixture = solo();
    let (ctx, _state) = installed(&fixture);
    let conn = ctx.conn_ctx(BrowserId::mint().expect("browser id"));
    let frames = ctx.hub().register(&conn);
    assert!(!has_summary(&ctx));

    fixtures::seal_document(
        &fixture.root,
        "plans/solo",
        &[("Solo does solo.", "plan-solo")],
    );
    let sealed = store::summary_path("plans/solo")
        .to_string_lossy()
        .into_owned();
    apply_batch(
        &ctx,
        &Batch {
            added: vec![sealed.into()],
            ..Batch::default()
        },
    );

    assert!(has_summary(&ctx));
    let checklist = std::iter::from_fn(|| frames.try_recv().ok())
        .find(|frame| frame.contains(r#""type":"checklist""#))
        .expect("the batch broadcasts the checklist");
    assert!(checklist.contains(r#""has_summary":true"#), "{checklist}");
}
