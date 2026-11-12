//! The vision summary page: served under its own page key, opening the vision's sections, while
//! the product overview still cites the vision directly.

use super::fixtures::{self as fx, Fixture};
use super::page::{load_page, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{Block, Citation, ConceptPage, EmptyKind, Sentence};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::VISION_ID;

const OVERVIEW: &str = "overview/demo/product";
const ANCHOR: &str = "specification-sections";

fn page(f: &Fixture, key: &PageKey) -> ConceptPage {
    let project = fx::load(&f.root);
    let eval = evaluate(&project, &[]);
    load_page(&project, &eval, key, &PageContext::default()).expect("the page exists")
}

fn vision_key() -> PageKey {
    PageKey::parse(VISION_ID).expect("the vision page key")
}

fn overview_key() -> PageKey {
    PageKey::Overview {
        feature: "demo".into(),
    }
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
fn the_vision_summary_page_serves_under_the_vision_key() {
    assert_eq!(
        vision_key(),
        PageKey::Summary {
            feature: Some("demo".into()),
            scope: Scope::Product,
            doc: "vision".into(),
        }
    );
    let f = fx::two_sections();
    let unbuilt = page(&f, &vision_key());
    assert_eq!(
        unbuilt.empty.expect("not built yet").kind,
        EmptyKind::NeverBuilt
    );

    fx::seal_document(
        &f.root,
        VISION_ID,
        &[("The vision lists sections.", ANCHOR)],
    );
    let built = page(&f, &vision_key());
    assert_eq!(built.key, VISION_ID);
    assert_eq!(built.feature.as_deref(), Some("demo"));
    assert_eq!(built.path.as_deref(), Some(fx::VISION));
    assert!(built.empty.is_none() && built.file_hash.is_some());

    let all = sentences(&built);
    assert_eq!(all.len(), 1);
    let source = Citation::Source {
        path: fx::VISION.into(),
        anchor: ANCHOR.into(),
    };
    assert_eq!(all[0].citations, [source]);
    assert_eq!(all[0].rests_on[0].path, fx::VISION);
    let mark = &all[0].marks[0];
    assert_eq!(
        (mark.doc.path.as_str(), mark.doc.page.as_str()),
        (fx::VISION, VISION_ID)
    );
    assert_eq!(mark.doc.id, VISION_ID);
}

#[test]
fn the_overview_still_cites_the_vision_directly_and_never_lists_it() {
    let f = fx::two_sections();
    fx::seal_document(&f.root, fx::ID1, &[("Section 1 holds.", "a")]);
    fx::seal_overview(
        &f.root,
        OVERVIEW,
        &[(fx::ID1, "Section 1 holds.")],
        &[(fx::VISION, ANCHOR)],
    );
    let overview = page(&f, &overview_key());
    let cited: Vec<_> = sentences(&overview)
        .into_iter()
        .flat_map(|one| &one.marks)
        .filter(|mark| mark.doc.path == fx::VISION)
        .collect();
    assert_eq!(cited.len(), 1, "one mark for the vision");
    assert_eq!(cited[0].colour, 0);
    assert_eq!(
        cited[0].doc.page, OVERVIEW,
        "the vision stands for its overview here"
    );
    assert!(
        cited[0].out_of_date.is_none(),
        "the missing vision summary dashes nothing"
    );
    assert!(overview
        .key_chips
        .iter()
        .all(|chip| chip.doc.path != fx::VISION));
    assert!(overview.unlisted.iter().all(|link| link.path != fx::VISION));
}
