//! Tests for sealing: overview members, preamble cites and archived plans.

use std::fs;
use std::path::Path;

use super::{seal, SealError};
use crate::planning::summary::evaluate::fixtures::{
    fixture, metaplan, put, two_sections, Fixture, ID1, ID2,
};
use crate::planning::summary::format::{parse, sentence_id, SummaryFile};
use crate::planning::summary::store::summary_path;

fn seal_text(f: &Fixture, id: &str, body: &str) -> Result<(), SealError> {
    let draft = f.paths.drafts.join(format!("{id}.md"));
    fs::create_dir_all(draft.parent().expect("parent")).expect("drafts dir");
    fs::write(&draft, body).expect("write draft");
    seal(&f.root, &f.paths, id, false).map(|_| ())
}

fn sealed_file(f: &Fixture, id: &str) -> SummaryFile {
    let text = fs::read_to_string(f.root.join(summary_path(id))).expect("summary file");
    parse(Path::new(&summary_path(id)), &text).expect("sealed summary parses")
}

#[test]
fn overview_members_follow_scope_order() {
    let f = two_sections();
    let (a, b) = ("Alpha one holds.", "Alpha two holds.");
    seal_text(
        &f,
        ID1,
        &format!("# S1\n\n{a} [s](../../../product/section_01.md#a)\n"),
    )
    .expect("s1");
    seal_text(
        &f,
        ID2,
        &format!("# S2\n\n{b} [s](../../../product/section_02.md#a)\n"),
    )
    .expect("s2");
    let overview = format!(
        "# Product\n\nOne. [m](product/section_01.md#{}) [m](product/section_02.md#{})\n\n\
         Vision. [v](../../product/index.md#specification-sections)\n",
        sentence_id(a),
        sentence_id(b)
    );
    seal_text(&f, "product", &overview).expect("overview");
    let members: Vec<String> = sealed_file(&f, "product")
        .front
        .members
        .into_keys()
        .collect();
    assert_eq!(members, vec![ID1.to_string(), ID2.to_string()]);
}

#[test]
fn preamble_cite_has_no_fragment() {
    let f = two_sections();
    seal_text(
        &f,
        ID1,
        "# S1\n\nIntro holds. [s](../../../product/section_01.md)\n",
    )
    .expect("seal");
    let cites = sealed_file(&f, ID1).front.cites;
    let keys: Vec<&String> = cites.keys().collect();
    assert_eq!(keys, vec!["doc/product/section_01.md"]);
}

#[test]
fn archived_plan_named_by_metaplan_seals() {
    let meta = metaplan(&["DONE-PLAN-b.md"]);
    let f = fixture(&[
        ("doc/plans/index.md", &meta),
        ("doc/plans/archive/DONE-PLAN-b.md", "# Plan: B\n\nDoes b.\n"),
    ]);
    let body = "# B\n\nB does b. [p](../../../plans/archive/DONE-PLAN-b.md#plan-b)\n";
    seal_text(&f, "plans/b", body).expect("archived plan sealed");
    assert_eq!(
        sealed_file(&f, "plans/b").front.of,
        "doc/plans/archive/DONE-PLAN-b.md"
    );
}

#[test]
fn seal_removes_draft_and_rejects_foreign_source() {
    let f = two_sections();
    put(&f.root, "doc/product/other.md", "# Other\n");
    let bad = "# S1\n\nOther holds. [s](../../../product/other.md)\n";
    let err = seal_text(&f, ID1, bad).expect_err("foreign source sealed");
    assert!(
        matches!(&err, SealError::Rejected(e) if e[0].line == 3),
        "{err}"
    );
    assert!(f.paths.drafts.join(format!("{ID1}.md")).exists());
    let good = "# S1\n\nAlpha holds. [s](../../../product/section_01.md#a)\n";
    seal_text(&f, ID1, good).expect("seal");
    assert!(!f.paths.drafts.join(format!("{ID1}.md")).exists());
}
