//! Tests for the concept routes and the engine driver.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::engine::ConceptState;
use super::fixtures::{self, Fixture, ID1};
use super::page::{load_page, page_key_of, PageContext, PageKey};
use super::{install, page_keys, routes};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::BrowserId;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::start_with_routes;
use crate::plan_server::tests::support::{bootstrap, get, json_body, post_json, skip, status};
use crate::plan_server::wire::concept::{Block, Citation, Sentence};
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::plan_server::ServerOptions;
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::store;
use crate::planning::watch::{Batch, OwnWrites};

fn served(fixture: &Fixture, read_only: ReadOnly) -> crate::plan_server::RunningServer {
    let options = ServerOptions {
        root: fixture.root.clone(),
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(0),
        read_only,
    };
    start_with_routes(options, routes()).expect("start the planning server")
}

fn open() -> ReadOnly {
    ReadOnly::from_problems(Vec::new())
}

#[test]
fn overview_200_with_chips() {
    if skip("overview_200_with_chips") {
        return;
    }
    let fixture = fixtures::two_sections();
    let server = served(&fixture, open());
    let cookies = bootstrap(&server);
    let response = get(&server, "/api/concept/overview/demo/product", &cookies);
    assert_eq!(status(&response), 200, "{response}");
    let page = json_body(&response);
    assert_eq!(page["feature"], "demo");
    assert_eq!(page["key"], "overview/demo/product");
    assert_eq!(page["key_chips"].as_array().map(Vec::len), Some(2));
}

#[test]
fn viewer_gets_200() {
    if skip("viewer_gets_200") {
        return;
    }
    let fixture = fixtures::two_sections();
    let problem = MissingPrereq {
        what: PrereqKind::Tmux,
        message: "tmux is not on PATH".to_owned(),
        fix: "Install tmux".to_owned(),
    };
    let server = served(&fixture, ReadOnly::from_problems(vec![problem]));
    let cookies = bootstrap(&server);
    assert_eq!(
        status(&get(
            &server,
            "/api/concept/overview/demo/product",
            &cookies
        )),
        200
    );
}

#[test]
fn unknown_section_404() {
    if skip("unknown_section_404") {
        return;
    }
    let server = served(&fixtures::two_sections(), open());
    let cookies = bootstrap(&server);
    let response = get(&server, "/api/concept/demo/product/section_99", &cookies);
    assert_eq!(status(&response), 404, "{response}");
}

#[test]
fn dot_dot_key_400() {
    if skip("dot_dot_key_400") {
        return;
    }
    let server = served(&fixtures::two_sections(), open());
    let cookies = bootstrap(&server);
    let response = get(&server, "/api/concept/demo/product/../x", &cookies);
    assert_eq!(status(&response), 400, "{response}");
    assert_eq!(json_body(&response)["error"], "bad_page");
}

#[test]
fn non_product_overviews_400() {
    if skip("non_product_overviews_400") {
        return;
    }
    let server = served(&fixtures::two_sections(), open());
    let cookies = bootstrap(&server);
    for scope in ["engineering", "metaplan"] {
        let path = format!("/api/concept/overview/demo/{scope}");
        let response = get(&server, &path, &cookies);
        assert_eq!(status(&response), 400, "{response}");
        assert_eq!(json_body(&response)["error"], "bad_page");
    }
}

#[test]
fn post_refused() {
    if skip("post_refused") {
        return;
    }
    let server = served(&fixtures::two_sections(), open());
    let cookies = bootstrap(&server);
    let response = post_json(
        &server,
        "/api/concept/overview/demo/product",
        &cookies,
        "{}",
    );
    assert!(matches!(status(&response), 404 | 405), "{response}");
}

fn context(fixture: &Fixture) -> Arc<PlanContext> {
    let local = SocketAddr::from(([127, 0, 0, 1], 0));
    let bind = BindInfo::new(local, "t");
    let ctx = PlanContext::new(fixture.root.clone(), open(), bind, OwnWrites::default());
    Arc::new(ctx.expect("context"))
}

fn edit_batch(ctx: &PlanContext, rel: &str) -> Batch {
    let batch = Batch {
        changed: vec![rel.into()],
        ..Batch::default()
    };
    ctx.project_mut().refresh(&batch).expect("refresh");
    batch
}

#[test]
fn delta_once_then_none() {
    let fixture = fixtures::two_sections();
    fixtures::seal_document(&fixture.root, ID1, &[("Alpha holds.", "a")]);
    let ctx = context(&fixture);
    install(&ctx);
    let state = ctx.extensions().get::<ConceptState>().expect("state");
    let conn = ctx.conn_ctx(BrowserId::mint().expect("browser id"));
    let frames = ctx.hub().register(&conn);

    let text = fixtures::section(1, "Changed.", "Beta.");
    fixtures::put(&fixture.root, fixtures::S1, &text);
    let batch = edit_batch(&ctx, fixtures::S1);
    let changed = state
        .handle(&ctx, &batch)
        .into_iter()
        .next()
        .expect("a delta");
    assert_eq!(
        changed.pages,
        ["demo/product/section_01", "overview/demo/product"]
    );
    assert_eq!(changed.out_of_date, [ID1]);
    assert_eq!(changed.sources, [fixtures::S1]);
    assert!(frames
        .try_recv()
        .expect("one frame")
        .contains("concept_changed"));

    assert!(state.handle(&ctx, &Batch::default()).is_empty());
    assert!(frames.try_recv().is_err());
}

#[test]
fn resealed_summary_with_unchanged_state_is_in_pages() {
    let fixture = fixtures::two_sections();
    fixtures::seal_document(&fixture.root, ID1, &[("Alpha holds.", "a")]);
    let ctx = context(&fixture);
    install(&ctx);
    let state = ctx.extensions().get::<ConceptState>().expect("state");
    fixtures::seal_document(&fixture.root, ID1, &[("Alpha still holds.", "a")]);
    let changed = state
        .handle(&ctx, &Batch::default())
        .into_iter()
        .next()
        .expect("a delta");
    assert_eq!(
        changed.pages,
        ["demo/product/section_01", "overview/demo/product"]
    );
    assert!(changed.out_of_date.is_empty());
}

#[test]
fn subscribe_receives_engine_update() {
    let fixture = fixtures::two_sections();
    let ctx = context(&fixture);
    install(&ctx);
    let state = ctx.extensions().get::<ConceptState>().expect("state");
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&seen);
    state.subscribe(Box::new(move |_, update| {
        counter.fetch_add(update.evaluation.summaries.len(), Ordering::SeqCst);
    }));
    state.handle(&ctx, &Batch::default());
    assert!(seen.load(Ordering::SeqCst) >= 2);
}

#[test]
fn engine_error_keeps_the_previous_evaluation() {
    let fixture = fixtures::two_sections();
    let ctx = context(&fixture);
    install(&ctx);
    let state = ctx.extensions().get::<ConceptState>().expect("state");
    let before = state.latest();
    let result = Err(anyhow::anyhow!("claims unreadable"));
    assert!(state.apply(&ctx, &Batch::default(), result).is_empty());
    assert!(Arc::ptr_eq(&before, &state.latest()));
}

#[test]
fn page_keys_for_a_summary() {
    let fixture = fixtures::fixture(&[
        (fixtures::VISION, &fixtures::vision(&[1])),
        (fixtures::S1, "# One\n"),
        ("doc/product/demo/section_03.md", "# Three\n"),
        (fixtures::META, &fixtures::metaplan(&["PLAN-a.md"])),
        ("doc/plans/demo/PLAN-a.md", "# A\n"),
    ]);
    let project = fixtures::load(&fixture.root);
    let keys = |id: &str| page_keys(id, &store::scope(&project, "demo"));
    assert_eq!(
        keys("demo/product/section_01"),
        ["demo/product/section_01", "overview/demo/product"]
    );
    assert_eq!(keys("demo/product/section_03"), ["demo/product/section_03"]);
    assert_eq!(keys("plans/a"), ["demo/engineering/a"]);
    assert!(keys("plans/zzz").is_empty());
    assert_eq!(keys("demo/metaplan/index"), ["demo/metaplan/index"]);
    assert!(keys("other/metaplan/index").is_empty());
    assert_eq!(keys("overview/demo/product"), ["overview/demo/product"]);
    for scope in ["engineering", "metaplan"] {
        assert!(keys(&format!("overview/demo/{scope}")).is_empty());
    }
}

/// The sentences of `blocks`.
fn sentences_of(blocks: &[Block]) -> Vec<&Sentence> {
    let paragraphs = blocks.iter().filter_map(|block| match block {
        Block::Paragraph { sentences } => Some(sentences),
        Block::Heading { .. } => None,
    });
    paragraphs.flatten().collect()
}

#[test]
fn a_citation_of_a_summary_whose_document_is_gone_is_dropped_and_the_rest_load() {
    let fixture = fixtures::two_sections();
    fixtures::seal_document(&fixture.root, ID1, &[("Section one holds.", "a")]);
    fixtures::seal_document(&fixture.root, fixtures::ID2, &[("Section two holds.", "a")]);
    fixtures::seal_overview(
        &fixture.root,
        "overview/demo/product",
        &[
            (ID1, "Section one holds."),
            (fixtures::ID2, "Section two holds."),
        ],
        &[],
    );
    std::fs::remove_file(fixture.root.join(fixtures::S1)).expect("remove the section");

    let project = fixtures::load(&fixture.root);
    let eval = evaluate(&project, &[]);
    let key = PageKey::parse("overview/demo/product").expect("key");
    let page = load_page(&project, &eval, &key, &PageContext::default()).expect("page");
    let cited: Vec<&Citation> = sentences_of(&page.blocks)
        .into_iter()
        .flat_map(|sentence| &sentence.citations)
        .collect();
    let summaries: Vec<&str> = cited
        .iter()
        .filter_map(|citation| match citation {
            Citation::Sentence { summary, .. } => Some(summary.as_str()),
            Citation::Source { .. } => None,
        })
        .collect();
    assert_eq!(
        summaries,
        [fixtures::ID2],
        "section one has no page to load"
    );

    if skip("a_citation_of_a_summary_whose_document_is_gone") {
        return;
    }
    let server = served(&fixture, open());
    let cookies = bootstrap(&server);
    for summary in summaries {
        // As the client does: the mark's page, else the summary id itself.
        let page = page_key_of(&project, summary).unwrap_or_else(|| summary.to_owned());
        let response = get(&server, &format!("/api/concept/{page}"), &cookies);
        assert_eq!(status(&response), 200, "{page}: {response}");
    }
}
