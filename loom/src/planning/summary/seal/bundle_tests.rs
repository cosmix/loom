//! Sealing a plan summary over its brief bundle: the `bundle` front matter is computed, never
//! taken from the draft, and a brief with no text to cite is hashed without being required.

use std::path::Path;

use super::SealError;
use crate::planning::summary::evaluate::fixtures::{fixture, metaplan, seal_draft, Fixture, META};
use crate::planning::summary::format::{self, sentence_id};
use crate::planning::summary::store::{link_from_summary, summary_path};
use crate::planning::summary::write::read_summary;

const ID: &str = "plans/x";
const PX: &str = "doc/plans/demo/PLAN-x.md";
const COMMON: &str = "doc/plans/demo/briefs/x/common.md";
const SCHEMA: &str = "doc/plans/demo/briefs/x/schema.json";
const PLAN: &str = "# Plan: X\n\n## Risks\n\nSchema drift.\n";
const D1: &str = "The schema may drift.";
const O1: &str = "X watches its schema.";

/// A Source link from plans/x to `rel#anchor`.
fn cite(rel: &str, anchor: &str) -> String {
    format!("[s]({}#{anchor})", link_from_summary(ID, Path::new(rel)))
}

/// A two-level draft whose one detail sentence cites the plan's risks and `extra`.
fn draft(extra: &str) -> String {
    let own = format!("[d](#{})", sentence_id(D1));
    format!("# X\n\n{O1} {own}\n\n{D1} {} {extra}\n", cite(PX, "risks"))
}

fn plan_with(briefs: &[(&str, &str)]) -> Fixture {
    let meta = metaplan(&["PLAN-x.md"]);
    let mut files = vec![(META, meta.as_str()), (PX, PLAN)];
    files.extend_from_slice(briefs);
    fixture(&files)
}

/// The sealed plans/x, parsed.
fn sealed_file(f: &Fixture) -> format::SummaryFile {
    let text = read_summary(&f.root, ID)
        .expect("read the summary")
        .expect("the summary is stored");
    format::parse(&summary_path(ID), &text)
        .unwrap_or_else(|errors| panic!("the sealed summary does not parse: {errors:?}\n{text}"))
}

#[test]
fn a_draft_carrying_its_own_bundle_is_refused() {
    let f = plan_with(&[(COMMON, "# Common\n\n## Rules\n\nRule one.\n")]);
    let forged = format!(
        "---\nbundle:\n  \"{COMMON}\": \"sha256:forged\"\n---\n{}",
        draft(&cite(COMMON, "rules"))
    );
    match seal_draft(&f, ID, &forged) {
        Err(SealError::Rejected(errors)) => assert!(
            errors
                .iter()
                .any(|e| e.line == 1 && e.message.contains("without front matter")),
            "{errors:?}"
        ),
        other => panic!("a draft with its own bundle was not refused: {other:?}"),
    }
    assert!(!f.root.join(summary_path(ID)).exists());
}

#[test]
fn a_brief_without_text_is_hashed_but_need_not_be_cited() {
    let f = plan_with(&[
        (COMMON, "# Common\n\n## Rules\n\nRule one.\n"),
        (SCHEMA, "{\"type\": \"object\"}\n"),
    ]);
    if let Err(error) = seal_draft(&f, ID, &draft(&cite(COMMON, "rules"))) {
        panic!("the draft citing every Markdown brief was refused: {error}");
    }
    let file = sealed_file(&f);
    let keys: Vec<&str> = file.front.bundle.keys().map(String::as_str).collect();
    assert_eq!(keys, [COMMON, SCHEMA]);
    assert!(file.front.bundle.values().all(|hash| !hash.is_empty()));
}

#[test]
fn a_plan_without_briefs_writes_no_bundle() {
    let f = plan_with(&[]);
    if let Err(error) = seal_draft(&f, ID, &draft("")) {
        panic!("the plan summary without briefs was refused: {error}");
    }
    let text = read_summary(&f.root, ID)
        .expect("read the summary")
        .expect("the summary is stored");
    assert!(!text.contains("bundle"), "{text}");
    assert!(sealed_file(&f).front.bundle.is_empty());
}
