//! The metaplan summary page: served under `<feature>/metaplan/<stem>`, listed in no overview,
//! while the engineering overview still cites the metaplan's passages directly.

use super::fixtures::{self as fx, Fixture};
use super::page::{load_page, page_key_of, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{Block, Citation, ConceptPage, EmptyKind, Sentence};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::METAPLAN_ID;

const ENGINEERING: &str = "overview/demo/engineering";
const PLAN_X: &str = "doc/plans/demo/PLAN-x.md";
const ANCHOR: &str = "sequencing";

fn fixture() -> Fixture {
    let meta = format!(
        "{}## Sequencing\n\nX first.\n",
        fx::metaplan(&["PLAN-x.md"])
    );
    fx::fixture(&[(fx::META, &meta), (PLAN_X, "# Plan: X\n\nDoes x.\n")])
}

fn page(f: &Fixture, key: &PageKey) -> ConceptPage {
    let project = fx::load(&f.root);
    let eval = evaluate(&project, &[]);
    load_page(&project, &eval, key, &PageContext::default()).expect("the page exists")
}

fn sentences(page: &ConceptPage) -> Vec<&Sentence> {
    page.blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { sentences } => Some(sentences),
            Block::Heading { .. } => None,
        })
        .flatten()
        .collect()
}

#[test]
fn the_metaplan_key_parses_and_formats_back_to_the_same_string() {
    let key = PageKey::parse(METAPLAN_ID).expect("the metaplan page key");
    assert_eq!(
        key,
        PageKey::Summary {
            feature: Some("demo".into()),
            scope: Scope::Metaplan,
            doc: "index".into(),
        }
    );
    assert_eq!(key.as_key(), METAPLAN_ID);
    assert!(
        PageKey::parse("overview/demo/metaplan").is_none(),
        "no overview has the metaplan scope"
    );
    assert!(PageKey::parse("demo/metaplan/a/b").is_none());
}

#[test]
fn the_metaplan_summary_page_serves_under_the_metaplan_key() {
    let f = fixture();
    let key = PageKey::parse(METAPLAN_ID).unwrap();
    let project = fx::load(&f.root);
    assert_eq!(
        page_key_of(&project, METAPLAN_ID).as_deref(),
        Some(METAPLAN_ID)
    );
    let unbuilt = page(&f, &key);
    assert_eq!(
        unbuilt.empty.expect("not built yet").kind,
        EmptyKind::NeverBuilt
    );

    fx::seal_document(&f.root, METAPLAN_ID, &[("X comes first.", ANCHOR)]);
    let built = page(&f, &key);
    assert_eq!(built.key, METAPLAN_ID);
    assert_eq!(built.path.as_deref(), Some(fx::META));
    assert!(built.empty.is_none() && built.file_hash.is_some());
    let all = sentences(&built);
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].citations,
        [Citation::Source {
            path: fx::META.into(),
            anchor: ANCHOR.into(),
        }]
    );
    assert_eq!(all[0].rests_on[0].path, fx::META);
    assert_eq!(all[0].marks[0].doc.page, METAPLAN_ID);
    assert!(built.unlisted.is_empty());
}

#[test]
fn the_engineering_overview_still_cites_the_metaplan_as_itself() {
    let f = fixture();
    fx::seal_document(&f.root, "plans/x", &[("X does x.", "plan-x")]);
    fx::seal_document(&f.root, METAPLAN_ID, &[("X comes first.", ANCHOR)]);
    fx::seal_overview(
        &f.root,
        ENGINEERING,
        &[("plans/x", "X does x.")],
        &[(fx::META, ANCHOR)],
    );
    let overview = page(
        &f,
        &PageKey::Overview {
            feature: "demo".into(),
            scope: Scope::Engineering,
        },
    );
    let cited: Vec<_> = sentences(&overview)
        .into_iter()
        .flat_map(|one| &one.marks)
        .filter(|mark| mark.doc.path == fx::META)
        .collect();
    assert_eq!(cited.len(), 1, "one mark for the metaplan");
    assert_eq!(cited[0].doc.page, ENGINEERING);
    assert_eq!(cited[0].doc.id, ENGINEERING);
    assert!(cited[0].out_of_date.is_none());
    assert!(overview
        .key_chips
        .iter()
        .all(|chip| chip.doc.path != fx::META));
    assert!(overview.unlisted.iter().all(|link| link.path != fx::META));
}
