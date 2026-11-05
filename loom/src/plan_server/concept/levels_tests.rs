//! Plan pages of one and two levels: the detail level travels only with a two-level summary, and
//! an opening sentence rests on the passages its detail sentences cite.

use super::fixtures::{fixture, load, metaplan, seal_document, seal_draft, Fixture, META};
use super::page::{load_page, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{Block, Citation, ConceptPage, Sentence};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::format::sentence_id;

const PX: &str = "doc/plans/demo/PLAN-x.md";
const LINK: &str = "../../../plans/demo/PLAN-x.md";
const PLAN: &str = "# Plan: X\n\n## Stages\n\nParse first.\n\n## Risks\n\nSchema drift.\n";

fn plan_fixture() -> Fixture {
    fixture(&[(META, &metaplan(&["PLAN-x.md"])), (PX, PLAN)])
}

fn plan_page(f: &Fixture) -> ConceptPage {
    let project = load(&f.root);
    let key = PageKey::Summary {
        feature: Some("demo".into()),
        scope: Scope::Engineering,
        doc: "x".into(),
    };
    let evaluation = evaluate(&project, &[]);
    load_page(&project, &evaluation, &key, &PageContext::default()).expect("the plan page exists")
}

fn shown(page: &ConceptPage) -> Vec<&Sentence> {
    let paragraphs = page.blocks.iter().filter_map(|block| match block {
        Block::Paragraph { sentences } => Some(sentences),
        Block::Heading { .. } => None,
    });
    paragraphs.flatten().collect()
}

fn anchors(sentence: &Sentence) -> Vec<(&str, &str)> {
    let rests = sentence.rests_on.iter();
    rests
        .map(|r| (r.path.as_str(), r.anchor.as_str()))
        .collect()
}

#[test]
fn a_one_level_plan_page_has_no_detail_level() {
    let f = plan_fixture();
    seal_document(&f.root, "plans/x", &[("X parses first.", "stages")]);
    let page = plan_page(&f);
    assert_eq!(shown(&page).len(), 1);
    assert!(page.detail.is_empty(), "{:?}", page.detail);
    let json = serde_json::to_value(&page).expect("serialize the page");
    assert!(json.get("detail").is_none(), "{json}");
}

#[test]
fn an_opening_sentence_rests_on_its_detail_passages_and_the_detail_on_its_own() {
    let f = plan_fixture();
    let (d1, d2) = ("Parsing runs first.", "The schema may drift.");
    let (o1, o2) = ("X starts with its parser.", "X watches its inputs.");
    let draft = format!(
        "# X\n\n{o1} [d](#{})\n{o2} [d](#{})\n\n{d1} [s]({LINK}#stages)\n\n\
         {d2} [s]({LINK}#risks) [s]({LINK}#stages)\n",
        sentence_id(d1),
        sentence_id(d2)
    );
    if let Err(error) = seal_draft(&f, "plans/x", &draft) {
        panic!("the two-level draft was refused: {error}");
    }
    let page = plan_page(&f);
    let opening = shown(&page);
    let ids: Vec<&str> = opening.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, [sentence_id(o1), sentence_id(o2)]);
    assert_eq!(anchors(opening[0]), [(PX, "stages")]);
    assert_eq!(anchors(opening[1]), [(PX, "risks"), (PX, "stages")]);
    let own = Citation::Sentence {
        summary: "plans/x".into(),
        id: sentence_id(d2),
    };
    assert_eq!(opening[1].citations, [own]);
    assert_eq!(opening[1].marks.len(), 1, "the plan's own mark");
    assert_eq!(opening[1].marks[0].doc.path, PX);
    let detail: Vec<&str> = page.detail.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(detail, [sentence_id(d1), sentence_id(d2)]);
    assert_eq!(anchors(&page.detail[1]), [(PX, "risks"), (PX, "stages")]);
    assert_eq!(page.detail[1].marks[0].doc.path, PX);
}
