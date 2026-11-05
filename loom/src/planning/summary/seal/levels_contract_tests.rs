//! Contracts for two-level plan summaries: a plan summary opens with sentences that cite whole
//! paragraphs of its own detail level, the detail level cites the plan, and a sealed two-level
//! summary evaluates current until a cited plan section changes.

use std::fs;

use super::{seal, SealError};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::{fixture, load, metaplan, put, Fixture, META};
use crate::planning::summary::evaluate::{SentenceState, SummaryState};
use crate::planning::summary::format::sentence_id;
use crate::planning::summary::store::summary_path;

const ID: &str = "plans/x";
const PX: &str = "doc/plans/demo/PLAN-x.md";
const PLAN: &str = "# Plan: X\n\n## Stages\n\nParse first.\n\n## Risks\n\nSchema drift.\n";
const D1: &str = "Parsing runs first.";
const D2: &str = "The schema may drift.";
const O1: &str = "X parses first and watches the schema.";
const STAGES: &str = "[s](../../../plans/demo/PLAN-x.md#stages)";
const RISKS: &str = "[s](../../../plans/demo/PLAN-x.md#risks)";

/// A feature whose metaplan lists `PLAN-x.md`, with Stages and Risks sections.
fn plan_fixture() -> Fixture {
    fixture(&[(META, &metaplan(&["PLAN-x.md"])), (PX, PLAN)])
}

/// A link from this summary to its own sentence whose text is `text`.
fn own(text: &str) -> String {
    format!("[d](#{})", sentence_id(text))
}

fn detail_lines() -> String {
    format!("{D1} {STAGES}\n{D2} {RISKS}\n")
}

fn opening_line() -> String {
    format!("{O1} {} {}", own(D1), own(D2))
}

fn one_level() -> String {
    format!("# X\n\n{}", detail_lines())
}

fn two_level_with(opening: &str) -> String {
    format!("# X\n\n{opening}\n\n{}", detail_lines())
}

fn two_level() -> String {
    two_level_with(&opening_line())
}

/// Write `draft` as the draft of plans/x and seal it.
fn seal_draft(f: &Fixture, draft: &str) -> Result<(), SealError> {
    let path = f.paths.drafts.join(format!("{ID}.md"));
    fs::create_dir_all(path.parent().expect("parent")).expect("drafts dir");
    fs::write(&path, draft).expect("write draft");
    seal(&f.root, &f.paths, ID, false).map(|_| ())
}

/// Seal `draft`, expect a rejection, and return its messages joined.
fn rejected(f: &Fixture, draft: &str) -> String {
    match seal_draft(f, draft) {
        Err(SealError::Rejected(errors)) => errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        other => panic!("expected a rejected seal of\n{draft}\ngot {other:?}"),
    }
}

fn stored(f: &Fixture) -> bool {
    f.root.join(summary_path(ID)).exists()
}

#[test]
fn a_plan_summary_seals_only_with_an_opening_level() {
    let f = plan_fixture();
    let messages = rejected(&f, &one_level());
    for detail in [D1, D2] {
        let id = sentence_id(detail);
        assert!(
            messages.contains(&id),
            "the refusal of a one-level plan summary names no detail id {id}: {messages}"
        );
    }
    assert!(!stored(&f), "a refused seal wrote the summary");
    if let Err(error) = seal_draft(&f, &two_level()) {
        panic!("the two-level plan summary was refused: {error}");
    }
    assert!(
        stored(&f),
        "the sealed two-level summary is not in the store"
    );
}

#[test]
fn an_opening_sentence_cites_whole_detail_paragraphs() {
    let f = plan_fixture();
    let half = two_level_with(&format!("{O1} {}", own(D1)));
    let chained = two_level_with(&format!(
        "{}\nX keeps a single parser. {}",
        opening_line(),
        own(O1)
    ));
    let with_source = two_level_with(&format!("{} {STAGES}", opening_line()));
    for (what, draft) in [
        ("cites half a detail paragraph", half),
        ("cites an opening sentence", chained),
        ("also cites the plan", with_source),
    ] {
        let messages = rejected(&f, &draft);
        assert!(!messages.is_empty(), "an opening sentence that {what}");
        assert!(
            !stored(&f),
            "a draft whose opening sentence {what} was stored"
        );
    }
    if let Err(error) = seal_draft(&f, &two_level()) {
        panic!("the two-level plan summary was refused: {error}");
    }
    assert!(
        stored(&f),
        "the sealed two-level summary is not in the store"
    );
}

/// The state of each sentence of plans/x, by sentence id.
fn sentence_states(f: &Fixture) -> (SummaryState, Vec<(String, SentenceState)>) {
    let evaluation = evaluate(&load(&f.root), &[]);
    let summary = &evaluation.summaries[ID];
    let states = summary
        .sentences
        .iter()
        .map(|s| (s.id.clone(), s.state.clone()))
        .collect();
    (summary.state.clone(), states)
}

#[test]
fn a_sealed_two_level_plan_summary_is_current_and_stales_through_its_detail() {
    let f = plan_fixture();
    if let Err(error) = seal_draft(&f, &two_level()) {
        panic!("the two-level plan summary was refused: {error}");
    }
    let (state, sentences) = sentence_states(&f);
    assert_eq!(state, SummaryState::Current, "sentences: {sentences:?}");
    assert_eq!(sentences.len(), 3, "sentences: {sentences:?}");
    for (id, state) in &sentences {
        assert_eq!(*state, SentenceState::Current, "sentence {id}");
    }

    put(&f.root, PX, &PLAN.replace("Parse first.", "Parse second."));
    let (_, sentences) = sentence_states(&f);
    let state_of = |text: &str| {
        let id = sentence_id(text);
        sentences
            .iter()
            .find(|(s, _)| *s == id)
            .map(|(_, state)| state.clone())
            .unwrap_or_else(|| panic!("no sentence {text:?} in {sentences:?}"))
    };
    assert_eq!(
        state_of(D1),
        SentenceState::Stale,
        "the detail citing Stages"
    );
    assert_eq!(state_of(O1), SentenceState::Stale, "the opening over D1");
    assert_eq!(
        state_of(D2),
        SentenceState::Current,
        "the detail citing Risks"
    );
}
