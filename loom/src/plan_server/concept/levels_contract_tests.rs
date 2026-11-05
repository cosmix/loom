//! Contract: a two-level plan summary's page shows its opening level in `blocks`, each opening
//! sentence citing its own detail sentences, and carries the detail level in `detail`.

use std::fs;

use super::fixtures::{fixture, load, metaplan, Fixture, META};
use super::page::{load_page, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{Block, Citation, Sentence};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::format::sentence_id;
use crate::planning::summary::seal::seal;

const ID: &str = "plans/x";
const PX: &str = "doc/plans/demo/PLAN-x.md";
const PLAN: &str = "# Plan: X\n\n## Stages\n\nParse first.\n\n## Risks\n\nSchema drift.\n";
const D1: &str = "Parsing runs first.";
const D2: &str = "The schema may drift.";
const O1: &str = "X parses first and watches the schema.";

/// The two-level draft of plans/x: O1 cites D1 and D2, which cite the plan's sections.
fn two_level() -> String {
    format!(
        "# X\n\n{O1} [d](#{}) [d](#{})\n\n\
         {D1} [s](../../../plans/demo/PLAN-x.md#stages)\n\
         {D2} [s](../../../plans/demo/PLAN-x.md#risks)\n",
        sentence_id(D1),
        sentence_id(D2)
    )
}

fn sealed_plan() -> Fixture {
    let f = fixture(&[(META, &metaplan(&["PLAN-x.md"])), (PX, PLAN)]);
    let path = f.paths.drafts.join(format!("{ID}.md"));
    fs::create_dir_all(path.parent().expect("parent")).expect("drafts dir");
    fs::write(&path, two_level()).expect("write draft");
    if let Err(error) = seal(&f.root, &f.paths, ID, false) {
        panic!("the two-level plan summary was refused: {error}");
    }
    f
}

fn own(text: &str) -> Citation {
    Citation::Sentence {
        summary: ID.into(),
        id: sentence_id(text),
    }
}

fn opening_sentences(blocks: &[Block]) -> Vec<&Sentence> {
    blocks
        .iter()
        .flat_map(|block| match block {
            Block::Paragraph { sentences } => sentences.iter().collect(),
            Block::Heading { .. } => Vec::new(),
        })
        .collect()
}

#[test]
fn a_plan_page_shows_its_opening_level_and_carries_its_detail() {
    let f = sealed_plan();
    let project = load(&f.root);
    let key = PageKey::Summary {
        feature: Some("demo".into()),
        scope: Scope::Engineering,
        doc: "x".into(),
    };
    let page = load_page(
        &project,
        &evaluate(&project, &[]),
        &key,
        &PageContext::default(),
    )
    .expect("the plan page exists");

    let opening = opening_sentences(&page.blocks);
    assert_eq!(
        opening.len(),
        1,
        "blocks hold the opening level only: {opening:?}"
    );
    let o = opening[0];
    assert_eq!(o.id, sentence_id(O1));
    assert_eq!(o.citations, vec![own(D1), own(D2)]);
    let rests: Vec<(&str, &str)> = o
        .rests_on
        .iter()
        .map(|r| (r.path.as_str(), r.anchor.as_str()))
        .collect();
    assert_eq!(rests, vec![(PX, "stages"), (PX, "risks")]);
    assert_eq!(o.marks[0].doc.page, "demo/engineering/x");

    let detail: Vec<&str> = page.detail.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(detail, vec![sentence_id(D1), sentence_id(D2)]);
    let source = Citation::Source {
        path: PX.into(),
        anchor: "stages".into(),
    };
    assert_eq!(page.detail[0].citations, vec![source]);
    let json = serde_json::to_value(&page).expect("serialize the page");
    let wire = json["detail"]
        .as_array()
        .expect("detail serializes as an array");
    assert_eq!(wire.len(), 2);
}
