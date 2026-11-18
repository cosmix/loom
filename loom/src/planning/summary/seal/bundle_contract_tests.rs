//! Contracts for plan summaries over the brief bundle: a plan summary may cite every file of
//! its plan's bundle, must cite each of them, records the bundle's hashes at seal time, and goes
//! out of date when a brief changes or joins the bundle.

use std::path::Path;

use super::SealError;
use crate::planning::summary::brief::brief;
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::{
    fixture, load, metaplan, put, seal_draft, Fixture, META,
};
use crate::planning::summary::evaluate::{SentenceState, SummaryState};
use crate::planning::summary::format::{self, sentence_id, FormatError};
use crate::planning::summary::store::{link_from_summary, summary_path};
use crate::planning::summary::write::read_summary;

const ID: &str = "plans/x";
const PX: &str = "doc/plans/demo/PLAN-x.md";
const PY: &str = "doc/plans/demo/PLAN-y.md";
const COMMON: &str = "doc/plans/demo/briefs/x/common.md";
const W1: &str = "doc/plans/demo/briefs/x/build/w1.md";
const W2: &str = "doc/plans/demo/briefs/x/build/w2.md";
const Y_COMMON: &str = "doc/plans/demo/briefs/y/common.md";
const COMMON_TEXT: &str = "# Common\n\n## Rules\n\nRule one.\n";
const W1_TEXT: &str = "# W1\n\n## Task\n\nDo it.\n";
const D1: &str = "Parsing runs first.";
const D2: &str = "The schema may drift.";
const O1: &str = "X parses first and watches the schema.";

/// A plan with Stages and Risks headings and one stage, `build`, in its metadata block.
fn plan_text(title: &str) -> String {
    format!(
        "# Plan: {title}\n\n## Stages\n\nParse first.\n\n## Risks\n\nSchema drift.\n\n\
         <!-- loom METADATA -->\n```yaml\nloom:\n  version: 1\n  stages:\n    - id: build\n      \
         name: Build\n```\n<!-- END loom METADATA -->\n"
    )
}

/// Feature `demo` listing PLAN-x.md and PLAN-y.md; x's bundle holds common.md and build/w1.md,
/// y's bundle holds its own common.md.
fn bundle_fixture() -> Fixture {
    let (px, py) = (plan_text("X"), plan_text("Y"));
    fixture(&[
        (META, &metaplan(&["PLAN-x.md", "PLAN-y.md"])),
        (PX, &px),
        (PY, &py),
        (COMMON, COMMON_TEXT),
        (W1, W1_TEXT),
        (Y_COMMON, COMMON_TEXT),
    ])
}

/// A Source link from plans/x to `rel#anchor`.
fn cite(rel: &str, anchor: &str) -> String {
    format!("[s]({}#{anchor})", link_from_summary(ID, Path::new(rel)))
}

fn own(text: &str) -> String {
    format!("[d](#{})", sentence_id(text))
}

/// A two-level draft whose second detail sentence carries `d2_cites` after its plan citation.
fn draft_with(d2_cites: &str) -> String {
    let d1 = format!("{D1} {} {}", cite(PX, "stage:build"), cite(W1, "task"));
    let d2 = format!("{D2} {} {d2_cites}", cite(PX, "risks"));
    format!("# X\n\n{O1} {} {}\n\n{d1}\n{d2}\n", own(D1), own(D2))
}

/// The draft citing the stage, its worker brief and common.md.
fn full_draft() -> String {
    draft_with(&cite(COMMON, "rules"))
}

fn sealed(f: &Fixture, draft: &str) {
    if let Err(error) = seal_draft(f, ID, draft) {
        panic!("the plan summary citing its bundle was refused: {error}\n{draft}");
    }
}

/// Seal `draft`, expect a rejection, and return its problems.
fn rejected(f: &Fixture, draft: &str) -> Vec<FormatError> {
    match seal_draft(f, ID, draft) {
        Err(SealError::Rejected(errors)) => errors,
        other => panic!("expected a rejected seal of\n{draft}\ngot {other:?}"),
    }
}

fn stored(f: &Fixture) -> bool {
    f.root.join(summary_path(ID)).exists()
}

#[test]
fn a_plan_summary_seals_citing_its_briefs() {
    let f = bundle_fixture();
    sealed(&f, &full_draft());
    let text = read_summary(&f.root, ID)
        .expect("read the sealed summary")
        .expect("the sealed summary is in the store");
    let file = format::parse(&summary_path(ID), &text)
        .unwrap_or_else(|errors| panic!("the sealed summary does not parse: {errors:?}"));
    let keys: Vec<&str> = file.front.bundle.keys().map(String::as_str).collect();
    let mut expected = vec![COMMON, W1];
    expected.sort_unstable();
    assert_eq!(
        keys, expected,
        "the sealed bundle map must hold exactly the two brief paths:\n{text}"
    );
    assert!(
        file.front.bundle.values().all(|hash| !hash.is_empty()),
        "every bundle entry carries a content hash:\n{text}"
    );
}

#[test]
fn a_plan_summary_must_cite_every_brief() {
    let f = bundle_fixture();
    let problems = rejected(&f, &draft_with(""));
    assert!(
        problems
            .iter()
            .any(|p| p.line == 1 && p.message.contains(COMMON)),
        "no problem on line 1 names the uncited brief {COMMON}: {problems:?}"
    );
    assert!(!stored(&f), "a refused seal wrote the summary");
}

#[test]
fn another_plans_brief_is_not_a_source() {
    let f = bundle_fixture();
    sealed(&f, &full_draft());
    let draft = draft_with(&format!(
        "{} {}",
        cite(COMMON, "rules"),
        cite(Y_COMMON, "rules")
    ));
    let problems = rejected(&f, &draft);
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains(Y_COMMON) && p.message.contains("is not a source")),
        "citing plan y's brief {Y_COMMON} was not refused as not a source: {problems:?}"
    );
}

/// The state of plans/x and of its sentence whose text is `text`.
fn states(f: &Fixture, text: &str) -> (SummaryState, SentenceState) {
    let evaluation = evaluate(&load(&f.root), &[]);
    let summary = &evaluation.summaries[ID];
    let id = sentence_id(text);
    let sentence = summary
        .sentences
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no sentence {text:?} in plans/x"));
    (summary.state, sentence.state)
}

#[test]
fn a_brief_edit_leaves_the_plan_summary_out_of_date() {
    let f = bundle_fixture();
    sealed(&f, &full_draft());
    let (state, _) = states(&f, D1);
    assert_eq!(state, SummaryState::Current, "freshly sealed");

    put(&f.root, W1, &W1_TEXT.replace("Do it.", "Do it twice."));
    let (state, sentence) = states(&f, D1);
    assert_ne!(state, SummaryState::Current, "after editing a cited brief");
    assert_eq!(
        sentence,
        SentenceState::Stale,
        "the detail sentence citing the edited brief span"
    );

    put(&f.root, W1, W1_TEXT);
    put(&f.root, W2, "# W2\n\n## Task\n\nDo more.\n");
    let (state, _) = states(&f, D1);
    assert_eq!(
        state,
        SummaryState::Incomplete,
        "after a brief joined the bundle"
    );
}

#[test]
fn the_rebuild_brief_lists_the_bundle() {
    let f = bundle_fixture();
    let text = brief(&f.root, &f.paths, &[ID.to_owned()], None)
        .unwrap_or_else(|error| panic!("the rebuild brief for {ID} failed: {error:#}"));
    let sources = text
        .split_once("Sources:")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("the rebuild brief has no Sources:\n{text}"));
    for path in [PX, COMMON, W1] {
        assert!(
            sources.contains(path),
            "the rebuild brief's Sources omit {path}:\n{text}"
        );
    }
}
