//! Tests for the engine driver: its run at install, its failure path, and the pages its delta
//! names beyond the changed summaries.

use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;

use super::engine::{install_with, ConceptState};
use super::fixtures::{self, Fixture, ID1, META, S1};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::BrowserId;
use crate::plan_server::prereq::ReadOnly;
use crate::planning::summary::evaluate::SummaryState;
use crate::planning::summary::store;
use crate::planning::watch::{Batch, OwnWrites};

const PLAN_X: &str = "doc/plans/demo/PLAN-x.md";
const ARCHIVED_X: &str = "doc/plans/demo/archive/DONE-PLAN-x.md";

/// A context over `fixture` with concept mode installed on the fixture's scratch runtime
/// directory.
fn installed(fixture: &Fixture) -> (Arc<PlanContext>, Arc<ConceptState>) {
    let bind = BindInfo::new(SocketAddr::from(([127, 0, 0, 1], 0)), "t");
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(fixture.root.clone(), read_only, bind, OwnWrites::default());
    let ctx = Arc::new(ctx.expect("context"));
    install_with(&ctx, fixture.paths.clone());
    let state = ctx.extensions().get::<ConceptState>().expect("state");
    (ctx, state)
}

/// `batch` after refreshing the project with it, as the watcher does before the hooks run.
fn refreshed(ctx: &PlanContext, batch: Batch) -> Batch {
    ctx.project_mut().refresh(&batch).expect("refresh");
    batch
}

fn added(ctx: &PlanContext, rel: &str) -> Batch {
    let batch = Batch {
        added: vec![rel.into()],
        ..Batch::default()
    };
    refreshed(ctx, batch)
}

fn changed(ctx: &PlanContext, rel: &str) -> Batch {
    let batch = Batch {
        changed: vec![rel.into()],
        ..Batch::default()
    };
    refreshed(ctx, batch)
}

fn summary_text(fixture: &Fixture, id: &str) -> Option<String> {
    fs::read_to_string(fixture.root.join(store::summary_path(id))).ok()
}

#[test]
fn install_rewrites_a_summary_whose_source_was_renamed_before_start() {
    let plan = "# Plan: X\n\nDoes x.\n\n## Detail\n\nMore.\n";
    let fixture = fixtures::fixture(&[(META, &fixtures::metaplan(&["PLAN-x.md"])), (PLAN_X, plan)]);
    let lines = [("X does x.", "plan-x"), ("X has detail.", "")];
    fixtures::seal_document(&fixture.root, "plans/x", &lines);
    fs::create_dir_all(fixture.root.join("doc/plans/demo/archive")).expect("archive dir");
    fs::rename(fixture.root.join(PLAN_X), fixture.root.join(ARCHIVED_X)).expect("rename");
    let (_ctx, state) = installed(&fixture);
    let latest = state.latest();
    assert_eq!(latest.summaries["plans/x"].state, SummaryState::Current);
    let text = summary_text(&fixture, "plans/x").expect("summary file");
    assert!(text.contains(&format!("of: {ARCHIVED_X}")), "{text}");
}

#[test]
fn install_deletes_the_summary_of_a_source_archived_before_start() {
    let fixture = fixtures::fixture(&[
        (META, &fixtures::metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
        ("doc/plans/demo/PLAN-d.md", "# Plan: D\n\nDoes d.\n"),
    ]);
    fixtures::seal_document(&fixture.root, "plans/d", &[("D does d.", "plan-d")]);
    fs::create_dir_all(fixture.root.join("doc/plans/demo/archive")).expect("archive dir");
    let archived = fixture.root.join("doc/plans/demo/archive/DONE-PLAN-d.md");
    fs::rename(fixture.root.join("doc/plans/demo/PLAN-d.md"), archived).expect("rename");
    let (_ctx, state) = installed(&fixture);
    assert!(summary_text(&fixture, "plans/d").is_none());
    assert!(!state.latest().summaries.contains_key("plans/d"));
}

#[test]
fn engine_error_still_broadcasts_the_batch_sources() {
    let fixture = fixtures::two_sections();
    let (ctx, state) = installed(&fixture);
    let conn = ctx.conn_ctx(BrowserId::mint().expect("browser id"));
    let frames = ctx.hub().register(&conn);
    let before = state.latest();
    let batch = changed(&ctx, S1);
    let result = Err(anyhow::anyhow!("engine failed"));
    let delta = state
        .apply(&ctx, &batch, result)
        .into_iter()
        .next()
        .expect("a delta");
    assert!(delta.pages.is_empty());
    assert!(delta.out_of_date.is_empty());
    assert_eq!(delta.sources, [S1]);
    assert!(Arc::ptr_eq(&before, &state.latest()));
    let frame = frames.try_recv().expect("one frame");
    assert!(frame.contains("concept_changed"), "{frame}");
}

#[test]
fn unreadable_claims_evaluate_without_them_as_install_does() {
    let fixture = fixtures::two_sections();
    fixtures::seal_document(&fixture.root, ID1, &[("Alpha holds.", "a")]);
    fs::create_dir_all(&fixture.paths.dir).expect("runtime dir");
    fs::write(&fixture.paths.claims, "not a directory").expect("claims file");
    let (ctx, state) = installed(&fixture);
    assert_eq!(state.latest().summaries[ID1].state, SummaryState::Current);
    let text = fixtures::section(1, "Changed.", "Beta.");
    fixtures::put(&fixture.root, S1, &text);
    let batch = changed(&ctx, S1);
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    assert_eq!(
        delta.pages,
        ["demo/product/section_01", "overview/demo/product"]
    );
    assert_eq!(delta.out_of_date, [ID1]);
    assert_eq!(delta.sources, [S1]);
    assert_eq!(state.latest().summaries[ID1].state, SummaryState::Stale);
}

#[test]
fn a_plan_no_metaplan_names_changes_the_engineering_overview() {
    let fixture = fixtures::fixture(&[
        (META, &fixtures::metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
    ]);
    let (ctx, state) = installed(&fixture);
    fixtures::put(
        &fixture.root,
        "doc/plans/demo/PLAN-y.md",
        "# Plan: Y\n\nDoes y.\n",
    );
    let batch = added(&ctx, "doc/plans/demo/PLAN-y.md");
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    assert_eq!(
        delta.pages,
        ["demo/engineering/y", "overview/demo/engineering"]
    );
}

#[test]
fn the_first_product_section_changes_the_product_overview() {
    let fixture = fixtures::fixture(&[(PLAN_X, "# Plan: X\n\nDoes x.\n")]);
    let (ctx, state) = installed(&fixture);
    fixtures::put(&fixture.root, S1, &fixtures::section(1, "Alpha.", "Beta."));
    let batch = added(&ctx, S1);
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    assert!(
        delta
            .pages
            .iter()
            .any(|page| page == "overview/demo/product"),
        "{delta:?}"
    );
    assert!(delta.pages.iter().any(|page| page == ID1), "{delta:?}");
}

#[test]
fn an_implements_edit_changes_both_sections_and_the_product_key() {
    let fixture = fixtures::fixture(&[
        (fixtures::VISION, &fixtures::vision(&[1, 2])),
        (S1, &fixtures::section(1, "Alpha.", "Beta.")),
        (fixtures::S2, &fixtures::section(2, "Alpha.", "Beta.")),
        (META, &fixtures::metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nImplements: section_01\n\nDoes x.\n"),
    ]);
    let (ctx, state) = installed(&fixture);
    let text = "# Plan: X\n\nImplements: section_02\n\nDoes x.\n";
    fixtures::put(&fixture.root, PLAN_X, text);
    let batch = changed(&ctx, PLAN_X);
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    for page in [
        "overview/demo/product",
        ID1,
        fixtures::ID2,
        "demo/engineering/x",
    ] {
        assert!(delta.pages.iter().any(|p| p == page), "{page}: {delta:?}");
    }
}

/// A metaplan naming plans x and y, plus a third plan w the metaplan does not name yet.
fn three_plans() -> Fixture {
    fixtures::fixture(&[
        (META, &fixtures::metaplan(&["PLAN-x.md", "PLAN-y.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
        ("doc/plans/demo/PLAN-y.md", "# Plan: Y\n\nDoes y.\n"),
        ("doc/plans/demo/PLAN-w.md", "# Plan: W\n\nDoes w.\n"),
    ])
}

const ALL_THREE_PAGES: [&str; 4] = [
    "demo/engineering/w",
    "demo/engineering/x",
    "demo/engineering/y",
    "overview/demo/engineering",
];

#[test]
fn a_plan_added_to_the_metaplan_names_every_summary_page_of_the_scope() {
    let fixture = three_plans();
    let (ctx, state) = installed(&fixture);
    let listed = fixtures::metaplan(&["PLAN-x.md", "PLAN-y.md", "PLAN-w.md"]);
    fixtures::put(&fixture.root, META, &listed);
    let batch = changed(&ctx, META);
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    assert_eq!(delta.pages, ALL_THREE_PAGES);
}

#[test]
fn a_key_row_change_names_the_summary_pages_of_unlisted_plans_too() {
    let fixture = fixtures::fixture(&[
        (META, &fixtures::metaplan(&["PLAN-x.md", "PLAN-y.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
        ("doc/plans/demo/PLAN-y.md", "# Plan: Y\n\nDoes y.\n"),
        ("doc/plans/demo/PLAN-u.md", "# Plan: U\n\nDoes u.\n"),
        ("doc/plans/demo/PLAN-w.md", "# Plan: W\n\nDoes w.\n"),
    ]);
    let (ctx, state) = installed(&fixture);
    let listed = fixtures::metaplan(&["PLAN-x.md", "PLAN-y.md", "PLAN-w.md"]);
    fixtures::put(&fixture.root, META, &listed);
    let batch = changed(&ctx, META);
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    let named = [
        "demo/engineering/u",
        "demo/engineering/w",
        "demo/engineering/x",
        "demo/engineering/y",
        "overview/demo/engineering",
    ];
    assert_eq!(delta.pages, named);
}

#[test]
fn a_retitled_plan_names_every_summary_page_of_the_scope() {
    let fixture = three_plans();
    let listed = fixtures::metaplan(&["PLAN-x.md", "PLAN-y.md", "PLAN-w.md"]);
    fixtures::put(&fixture.root, META, &listed);
    let (ctx, state) = installed(&fixture);
    let plan = "doc/plans/demo/PLAN-y.md";
    fixtures::put(&fixture.root, plan, "# Plan: Renamed\n\nDoes y.\n");
    let batch = changed(&ctx, plan);
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    assert_eq!(delta.pages, ALL_THREE_PAGES);
}

#[test]
fn a_no_plan_flip_names_every_section_page_of_the_product_scope() {
    let section = |n| fixtures::section(n, "Alpha.", "Beta.");
    let fixture = fixtures::fixture(&[
        (fixtures::VISION, &fixtures::vision(&[1, 2, 3])),
        (S1, &section(1)),
        (fixtures::S2, &section(2)),
        ("doc/product/demo/section_03.md", &section(3)),
        (META, &fixtures::metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nImplements: section_01\n\nDoes x.\n"),
    ]);
    let (ctx, state) = installed(&fixture);
    let text = "# Plan: X\n\nImplements: section_02\n\nDoes x.\n";
    fixtures::put(&fixture.root, PLAN_X, text);
    let batch = changed(&ctx, PLAN_X);
    let delta = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    for page in [ID1, fixtures::ID2, "demo/product/section_03"] {
        assert!(delta.pages.iter().any(|p| p == page), "{page}: {delta:?}");
    }
}
