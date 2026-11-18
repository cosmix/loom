//! Evaluation of a two-level plan summary: an opening sentence holds through the current detail
//! sentences it cites, and nothing else.

use std::fs;
use std::path::Path;

use super::fixtures::{fixture, load, metaplan, put, seal_draft, Fixture, META};
use super::{evaluate, SentenceState, SummaryState};
use crate::planning::summary::format::sentence_id;
use crate::planning::summary::store::summary_path;

const ID: &str = "plans/x";
const PX: &str = "doc/plans/demo/PLAN-x.md";
const LINK: &str = "../../../plans/demo/PLAN-x.md";
const PLAN: &str = "# Plan: X\n\n## Stages\n\nParse first.\n\n## Risks\n\nSchema drift.\n\n\
                    ## Notes\n\nKeep it small.\n";
const O1: &str = "X parses before it worries about the schema.";
const O2: &str = "X keeps its scope narrow.";
const D1: &str = "Parsing runs first.";
const D2: &str = "The schema may drift.";
const D3: &str = "The plan stays small.";

fn own(text: &str) -> String {
    format!("[d](#{})", sentence_id(text))
}

/// plans/x sealed with O1 opening the paragraph of D1 and D2, and O2 opening D3's.
fn sealed() -> Fixture {
    let f = fixture(&[(META, &metaplan(&["PLAN-x.md"])), (PX, PLAN)]);
    let draft = format!(
        "# X\n\n{O1} {} {}\n{O2} {}\n\n{D1} [s]({LINK}#stages)\n{D2} [s]({LINK}#risks)\n\n\
         {D3} [s]({LINK}#notes)\n",
        own(D1),
        own(D2),
        own(D3)
    );
    if let Err(error) = seal_draft(&f, ID, &draft) {
        panic!("the two-level draft was refused: {error}");
    }
    f
}

/// The summary's state, its sentence count and the texts of its stale sentences in file order.
fn states(root: &Path) -> (SummaryState, usize, Vec<String>) {
    let evaluation = evaluate(&load(root), &[]);
    let summary = &evaluation.summaries[ID];
    let stale = summary
        .sentences
        .iter()
        .filter(|s| s.state == SentenceState::Stale)
        .map(|s| s.text.clone());
    (summary.state, summary.sentences.len(), stale.collect())
}

#[test]
fn a_sealed_two_level_plan_summary_is_current() {
    let f = sealed();
    let (state, count, stale) = states(&f.root);
    assert_eq!(state, SummaryState::Current, "stale: {stale:?}");
    assert_eq!(count, 5);
    assert!(stale.is_empty(), "{stale:?}");
}

#[test]
fn a_changed_detail_span_stales_its_sentence_and_the_opening_over_it_only() {
    let f = sealed();
    put(&f.root, PX, &PLAN.replace("Parse first.", "Parse second."));
    let (state, _, stale) = states(&f.root);
    assert_eq!(state, SummaryState::Stale);
    assert_eq!(stale, [O1, D1]);
}

#[test]
fn opening_sentences_citing_each_other_read_stale() {
    let f = sealed();
    let path = f.root.join(summary_path(ID));
    let text = fs::read_to_string(&path).expect("summary file");
    let o1 = format!("{O1} {} {}", own(D1), own(D2));
    let o2 = format!("{O2} {}", own(D3));
    let edited = text
        .replace(&o1, &format!("{o1} {}", own(O2)))
        .replace(&o2, &format!("{O2} {}", own(O1)));
    assert_ne!(edited, text, "the hand edit changed nothing");
    fs::write(&path, edited).expect("hand edit");
    let (_, count, stale) = states(&f.root);
    assert_eq!(count, 5);
    assert_eq!(stale, [O1, O2]);
}
