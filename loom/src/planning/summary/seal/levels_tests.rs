//! The seal's two-level rules for plan summaries, one test per rule, and what an overview may
//! cite of a two-level plan summary.

use super::SealError;
use crate::planning::summary::evaluate::fixtures::{
    fixture, metaplan, seal_draft, two_sections, Fixture, ENGINEERING, ID1, META,
};
use crate::planning::summary::format::sentence_id;

const ID: &str = "plans/x";
const PX: &str = "doc/plans/demo/PLAN-x.md";
const PLAN: &str = "# Plan: X\n\n## Stages\n\nParse first.\n\n## Risks\n\nSchema drift.\n";
const O1: &str = "X parses first and watches the schema.";
const D1: &str = "Parsing runs first.";
const D2: &str = "The schema may drift.";
const STAGES: &str = "[s](../../../plans/demo/PLAN-x.md#stages)";
const RISKS: &str = "[s](../../../plans/demo/PLAN-x.md#risks)";
const ORDER: &str = "the opening level comes first: headings and opening paragraphs end where the \
                     detail level begins";
const METAPLAN_LINK: &str = "[m](../../../../plans/demo/index.md#metaplan)";

fn plan_fixture() -> Fixture {
    fixture(&[(META, &metaplan(&["PLAN-x.md"])), (PX, PLAN)])
}

/// A link from plans/x to its own sentence whose text is `text`.
fn own(text: &str) -> String {
    format!("[d](#{})", sentence_id(text))
}

/// D1 and D2 as one detail paragraph.
fn detail() -> String {
    format!("{D1} {STAGES}\n{D2} {RISKS}\n")
}

/// `# X`, the opening paragraph `opening` on line 3, then the detail paragraph from line 5.
fn draft(opening: &str) -> String {
    format!("# X\n\n{opening}\n\n{}", detail())
}

fn detail_list() -> String {
    format!(
        "detail sentences: {} \"{D1}\", {} \"{D2}\"",
        sentence_id(D1),
        sentence_id(D2)
    )
}

/// The `line: message` problems of sealing `text` as summary `id`; anything else fails.
fn rejections(f: &Fixture, id: &str, text: &str) -> Vec<String> {
    match seal_draft(f, id, text) {
        Err(SealError::Rejected(errors)) => errors.iter().map(ToString::to_string).collect(),
        other => panic!("expected a rejected seal of\n{text}\ngot {other:?}"),
    }
}

fn sealed(f: &Fixture, id: &str, text: &str) {
    if let Err(error) = seal_draft(f, id, text) {
        panic!("{id} was refused: {error}\n{text}");
    }
}

#[test]
fn a_one_level_plan_draft_is_refused_with_its_detail_ids_and_first_words() {
    let f = plan_fixture();
    let long = "The schema may drift between releases of the parser.";
    let text = format!("# X\n\n{D1} {STAGES}\n{long} {RISKS}\n");
    let want = format!(
        "1: a plan summary opens with sentences that cite its detail sentences as [d](#s-…); \
         detail sentences: {} \"{D1}\", {} \"The schema may drift between releases\"",
        sentence_id(D1),
        sentence_id(long)
    );
    assert_eq!(rejections(&f, ID, &text), [want]);
}

#[test]
fn a_section_summary_still_cites_no_sentence_of_its_own() {
    let f = two_sections();
    let text = format!(
        "# S1\n\nS1 opens. {}\n\nAlpha holds. [s](../../../../product/demo/section_01.md#a)\n",
        own("Alpha holds.")
    );
    assert_eq!(
        rejections(&f, ID1, &text),
        ["3: a document summary cites only its source document"]
    );
}

#[test]
fn an_opening_sentence_cites_no_source() {
    let f = plan_fixture();
    let text = draft(&format!("{O1} {} {} {STAGES}", own(D1), own(D2)));
    let want = format!(
        "3: sentence {} cites both this summary and the plan; an opening sentence cites only \
         detail sentences",
        sentence_id(O1)
    );
    assert_eq!(rejections(&f, ID, &text), [want]);
}

#[test]
fn the_opening_level_comes_before_the_detail_level() {
    let f = plan_fixture();
    let opening = format!("{O1} {} {}", own(D1), own(D2));
    let mixed = format!("# X\n\n{opening}\n{}", detail());
    assert_eq!(rejections(&f, ID, &mixed), [format!("4: {ORDER}")]);
    let after = format!("# X\n\n{}\n{opening}\n", detail());
    assert_eq!(rejections(&f, ID, &after), [format!("6: {ORDER}")]);
    let heading = format!("# X\n\n{opening}\n\n{D1} {STAGES}\n\n## Risks\n\n{D2} {RISKS}\n");
    assert_eq!(rejections(&f, ID, &heading), [format!("7: {ORDER}")]);
    sealed(
        &f,
        ID,
        &format!("# X\n\n## Summary\n\n{opening}\n\n{}", detail()),
    );
}

#[test]
fn an_opening_sentence_cites_only_detail_sentences_of_this_summary() {
    let f = plan_fixture();
    let o1 = sentence_id(O1);
    for target in ["s-00000000".to_string(), o1.clone()] {
        let text = draft(&format!("{O1} {} {} [d](#{target})", own(D1), own(D2)));
        let want = format!(
            "3: sentence {o1} cites {target}, which is not a detail sentence of this summary; {}",
            detail_list()
        );
        assert_eq!(rejections(&f, ID, &text), [want], "a link to {target}");
    }
}

#[test]
fn an_opening_sentence_opens_whole_detail_paragraphs_in_order() {
    let f = plan_fixture();
    let part = format!(
        "3: sentence {} opens part of a detail paragraph; cite every sentence of each paragraph \
         it opens, in order",
        sentence_id(O1)
    );
    let half = draft(&format!("{O1} {}", own(D1)));
    let unopened = format!(
        "6: detail sentence {} is opened by no opening sentence",
        sentence_id(D2)
    );
    assert_eq!(rejections(&f, ID, &half), [part.clone(), unopened]);
    let reversed = draft(&format!("{O1} {} {}", own(D2), own(D1)));
    assert_eq!(rejections(&f, ID, &reversed), [part]);
    let two = format!(
        "# X\n\n{O1} {} {}\n\n{D1} {STAGES}\n\n{D2} {RISKS}\n",
        own(D1),
        own(D2)
    );
    sealed(&f, ID, &two);
}

#[test]
fn every_detail_sentence_is_opened() {
    let f = plan_fixture();
    let text = format!("# X\n\n{O1} {}\n\n{D1} {STAGES}\n\n{D2} {RISKS}\n", own(D1));
    let want = format!(
        "7: detail sentence {} is opened by no opening sentence",
        sentence_id(D2)
    );
    assert_eq!(rejections(&f, ID, &text), [want]);
}

#[test]
fn each_level_keeps_its_own_cap() {
    let f = plan_fixture();
    let details: Vec<String> = (1..=9)
        .map(|i| format!("Detail number {i} holds."))
        .collect();
    let openings: Vec<String> = details
        .iter()
        .enumerate()
        .map(|(i, d)| format!("Opening {} sums up a part. {}", i + 1, own(d)))
        .collect();
    let paragraphs: Vec<String> = details.iter().map(|d| format!("{d} {STAGES}\n")).collect();
    let text = format!(
        "# X\n\n{}\n\n{}",
        openings.join("\n"),
        paragraphs.join("\n")
    );
    assert_eq!(
        rejections(&f, ID, &text),
        ["11: more than 8 opening sentences"]
    );
    let details: Vec<String> = (1..=16)
        .map(|i| format!("Detail number {i} holds."))
        .collect();
    let links: Vec<String> = details.iter().map(|d| own(d)).collect();
    let lines: Vec<String> = details.iter().map(|d| format!("{d} {STAGES}")).collect();
    let text = format!(
        "# X\n\nX sums the details up. {}\n\n{}\n",
        links.join(" "),
        lines.join("\n")
    );
    assert_eq!(
        rejections(&f, ID, &text),
        ["20: more than 15 detail sentences"]
    );
}

#[test]
fn an_opening_sentence_does_not_restate_what_it_opens() {
    let f = plan_fixture();
    let restating = "Parsing runs first here.";
    let text = format!(
        "# X\n\n{restating} {}\nX watches for drift in its inputs. {}\n\n{D1} {STAGES}\n\n\
         {D2} {RISKS}\n",
        own(D1),
        own(D2)
    );
    let want = format!(
        "3: sentence {} restates detail sentence {}; say what the paragraph adds up to",
        sentence_id(restating),
        sentence_id(D1)
    );
    assert_eq!(rejections(&f, ID, &text), [want]);
}

/// An engineering overview draft whose first sentence cites `link` into plans/x.
fn overview(link: &str) -> String {
    format!(
        "# Engineering\n\nX is planned and ordered. {link}\n\nThe metaplan orders it. \
         {METAPLAN_LINK}\n"
    )
}

fn member(text: &str) -> String {
    format!("[m](../../plans/x.md#{})", sentence_id(text))
}

#[test]
fn an_overview_cites_a_plan_summarys_detail_and_never_its_opening() {
    let f = plan_fixture();
    sealed(&f, ID, &draft(&format!("{O1} {} {}", own(D1), own(D2))));
    let want = [
        "1: member summary plans/x is not cited".to_string(),
        format!(
            "3: sentence {} of plans/x opens to detail sentences; cite those detail sentences \
             instead",
            sentence_id(O1)
        ),
    ];
    assert_eq!(rejections(&f, ENGINEERING, &overview(&member(O1))), want);
    sealed(
        &f,
        ENGINEERING,
        &overview(&format!("{} {}", member(D1), member(D2))),
    );
}

#[test]
fn an_overview_may_cite_the_single_detail_sentence_of_a_plan_summary() {
    let f = fixture(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (PX, "# Plan: X\n\nDoes x.\n"),
    ]);
    let single = "X does x.";
    let plan = format!(
        "# X\n\nX is planned. {}\n\n{single} [§](../../../plans/demo/PLAN-x.md#plan-x)\n",
        own(single)
    );
    sealed(&f, ID, &plan);
    sealed(&f, ENGINEERING, &overview(&member(single)));
}
