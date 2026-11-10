//! Settling a document changes no summary hash: no summary goes out of date, the engine reports
//! nothing new and no summary's inputs move; an edit still does all three.

use std::fs;

use super::*;
use crate::planning::summary::engine::SummaryEngine;
use crate::planning::summary::evaluate::fixtures::*;
use crate::planning::summary::evaluate::{evaluate, SummaryState};
use crate::planning::trace::settle::{clear, mark, SettledLine};
use crate::planning::watch::{Batch, OwnWrites};

const PLAN: &str = "doc/plans/demo/PLAN-x.md";
const PLAN_ID: &str = "plans/x";
const DOCUMENTS: [&str; 4] = [VISION, S1, META, PLAN];

/// Every kind of summary sealed current, each citing the level-1 span that holds the line
/// settling writes: the vision, section 01, the metaplan, a plan implementing section 01, and
/// both overviews.
fn sealed() -> Fixture {
    let f = fixture(&[
        (VISION, &vision(&[1])),
        (S1, &section(1, "Alpha.", "Beta.")),
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN, "# Plan: X\n\nImplements: section_01\n\nDoes x.\n"),
    ]);
    seal_document(&f.root, VISION_ID, &[("The vision holds.", "vision")]);
    seal_document(&f.root, ID1, &[("Section one holds.", "section-01")]);
    seal_document(&f.root, METAPLAN_ID, &[("The metaplan holds.", "metaplan")]);
    seal_document(&f.root, PLAN_ID, &[("X does x.", "plan-x")]);
    let product = [(ID1, "Section one holds.")];
    seal_overview(&f.root, PRODUCT, &product, &[(VISION, "vision")]);
    let engineering = [(PLAN_ID, "X does x.")];
    seal_overview(&f.root, ENGINEERING, &engineering, &[(META, "metaplan")]);
    f
}

fn states(f: &Fixture) -> Vec<(String, SummaryState)> {
    let evaluation = evaluate(&load(&f.root), &[]);
    let summaries = evaluation.summaries.into_values();
    summaries.map(|eval| (eval.id, eval.state)).collect()
}

fn all_current(f: &Fixture) -> Vec<(String, SummaryState)> {
    let mut ids = [ENGINEERING, PRODUCT, METAPLAN_ID, ID1, VISION_ID, PLAN_ID];
    ids.sort_unstable();
    let want: Vec<(String, SummaryState)> = ids
        .iter()
        .map(|id| (id.to_string(), SummaryState::Current))
        .collect();
    assert_eq!(states(f), want, "the sealed fixture");
    want
}

/// Rewrite every document of the fixture with `edit`.
fn rewrite(f: &Fixture, edit: impl Fn(&str) -> String) {
    for path in DOCUMENTS {
        let text = fs::read_to_string(f.root.join(path)).expect("document");
        put(&f.root, path, &edit(&text));
    }
}

fn line(body: &str, stub: Option<&str>) -> SettledLine {
    SettledLine {
        body: body.to_owned(),
        stub: stub.map(str::to_owned),
        upstream: None,
    }
}

#[test]
fn settling_changing_and_clearing_the_settled_line_leaves_every_summary_current() {
    let f = sealed();
    let current = all_current(&f);
    rewrite(&f, |text| mark(text, &line("0123456789ab", None)));
    assert_eq!(states(&f), current, "settled");
    rewrite(&f, |text| {
        mark(text, &line("ba9876543210", Some("00000000000a")))
    });
    assert_eq!(states(&f), current, "settled again with other hashes");
    rewrite(&f, clear);
    assert_eq!(states(&f), current, "cleared");
}

#[test]
fn a_settle_invalidates_nothing_and_moves_no_inputs() {
    let f = sealed();
    all_current(&f);
    let mut engine = SummaryEngine::new(f.root.clone(), f.paths.clone(), OwnWrites::default());
    let before = engine
        .on_batch(&load(&f.root), &Batch::default())
        .expect("batch");
    let inputs = input_hashes(&load(&f.root), &before.evaluation);
    rewrite(&f, |text| mark(text, &line("0123456789ab", None)));
    let project = load(&f.root);
    let after = engine.on_batch(&project, &Batch::default()).expect("batch");
    assert_eq!(after.invalidated, Vec::<String>::new());
    assert_eq!(input_hashes(&project, &after.evaluation), inputs);
}

#[test]
fn an_edit_moves_the_inputs_of_its_summary_and_the_overview_over_it() {
    let f = sealed();
    let project = load(&f.root);
    let before = input_hashes(&project, &evaluate(&project, &[]));
    put(&f.root, S1, &section(1, "Alpha edited.", "Beta."));
    let project = load(&f.root);
    let after = input_hashes(&project, &evaluate(&project, &[]));
    let moved: Vec<&String> = after
        .keys()
        .filter(|id| after[*id] != before[*id])
        .collect();
    assert_eq!(moved, [ID1, PRODUCT]);
}

#[test]
fn a_document_without_a_settled_line_keeps_its_loaded_sections() {
    let f = sealed();
    let project = load(&f.root);
    let hashes = SourceHashes::new(&project);
    let loaded = &project.docs[std::path::Path::new(S1)].sections;
    assert_eq!(hashes.sections(std::path::Path::new(S1)), Some(loaded));
    assert_eq!(
        hashes.sections(std::path::Path::new("doc/missing.md")),
        None
    );
}
