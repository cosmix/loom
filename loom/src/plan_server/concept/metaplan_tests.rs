//! The metaplan summary page: served under `<feature>/metaplan/<stem>`, with plan key chips.

use super::fixtures::{self as fx, Fixture};
use super::page::{load_page, page_key_of, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{Block, Citation, ConceptPage, EmptyKind, Sentence};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::METAPLAN_ID;

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
fn only_product_overviews_parse_or_load() {
    let f = fixture();
    let project = fx::load(&f.root);
    let eval = evaluate(&project, &[]);
    for (scope, name) in [
        (Scope::Engineering, "engineering"),
        (Scope::Metaplan, "metaplan"),
    ] {
        assert!(PageKey::parse(&format!("overview/demo/{name}")).is_none());
        let key = PageKey::Overview {
            feature: "demo".into(),
            scope,
        };
        assert!(load_page(&project, &eval, &key, &PageContext::default()).is_none());
    }
    let product = PageKey::parse("overview/demo/product").expect("product overview key");
    assert!(load_page(&project, &eval, &product, &PageContext::default()).is_some());
}

#[test]
fn metaplan_links_and_chips_target_document_pages() {
    let f = fixture();
    let project = fx::load(&f.root);
    let link = super::body::doc_link(&project, std::path::Path::new(fx::META)).unwrap();
    assert_eq!(link.id, METAPLAN_ID);
    assert_eq!(link.page, METAPLAN_ID);
    let metaplan = page(&f, &PageKey::parse(METAPLAN_ID).unwrap());
    assert_eq!(metaplan.key_chips.len(), 1);
    assert_eq!(metaplan.key_chips[0].doc.page, "demo/engineering/x");
    assert_eq!(metaplan.key_chips[0].doc.id, "plans/x");
}
