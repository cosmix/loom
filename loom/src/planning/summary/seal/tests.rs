//! Tests for sealing: overview members, preamble cites, archived plans and link rejections.

use std::fs;
use std::path::Path;

use super::{seal, SealError};
use crate::planning::summary::evaluate::fixtures::{
    fixture, metaplan, put, seal_document, section, two_sections, Fixture, ENGINEERING, ID1, ID2,
    PRODUCT,
};
use crate::planning::summary::format::{parse, sentence_id, SummaryFile};
use crate::planning::summary::store::{link_from_summary, resolve_link, summary_path};

fn seal_text(f: &Fixture, id: &str, body: &str) -> Result<(), SealError> {
    let draft = f.paths.drafts.join(format!("{id}.md"));
    fs::create_dir_all(draft.parent().expect("parent")).expect("drafts dir");
    fs::write(&draft, body).expect("write draft");
    seal(&f.root, &f.paths, id, false).map(|_| ())
}

/// The `line: message` problems of a rejected seal; anything else fails the test.
fn rejections(result: Result<(), SealError>) -> Vec<String> {
    match result {
        Err(SealError::Rejected(errors)) => errors.iter().map(ToString::to_string).collect(),
        other => panic!("expected a rejected seal, got {other:?}"),
    }
}

const A1: &str = "Alpha one holds.";
const A2: &str = "Alpha two holds.";
const ID3: &str = "demo/product/section_03";
const VISION_LINK: &str = "[v](../../../../product/demo/index.md#specification-sections)";
const METAPLAN_LINK: &str = "[m](../../../../plans/demo/index.md#metaplan)";

/// `two_sections` with both document summaries sealed, so an overview draft carries only the
/// defect under test.
fn sealed_sections() -> Fixture {
    let f = two_sections();
    seal_document(&f.root, ID1, &[(A1, "a")]);
    seal_document(&f.root, ID2, &[(A2, "a")]);
    f
}

/// A metaplan listing `PLAN-b.md` and the sealed summary of that plan.
fn sealed_plan() -> Fixture {
    let meta = metaplan(&["PLAN-b.md"]);
    let f = fixture(&[
        ("doc/plans/demo/index.md", &meta),
        ("doc/plans/demo/PLAN-b.md", "# Plan: B\n\nDoes b.\n"),
    ]);
    seal_document(&f.root, "plans/b", &[("B does b.", "plan-b")]);
    f
}

/// A link from an overview to the sentence of member `id` whose text is `text`.
fn member(id: &str, text: &str) -> String {
    format!(
        "[m]({}#{})",
        link_from_summary(PRODUCT, &summary_path(id)),
        sentence_id(text)
    )
}

/// An overview draft with one sentence per link: sentence `i` is on line `3 + 2 * i`.
fn overview_draft(links: &[String]) -> String {
    let body: String = links
        .iter()
        .enumerate()
        .map(|(i, link)| format!("Point {i} holds. {link}\n\n"))
        .collect();
    format!("# Overview\n\n{body}")
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
        &format!("# S1\n\n{a} [s](../../../../product/demo/section_01.md#a)\n"),
    )
    .expect("s1");
    seal_text(
        &f,
        ID2,
        &format!("# S2\n\n{b} [s](../../../../product/demo/section_02.md#a)\n"),
    )
    .expect("s2");
    let overview = format!(
        "# Product\n\nOne. [m](../../demo/product/section_01.md#{}) [m](../../demo/product/section_02.md#{})\n\n\
         Vision. [v](../../../../product/demo/index.md#specification-sections)\n",
        sentence_id(a),
        sentence_id(b)
    );
    seal_text(&f, PRODUCT, &overview).expect("overview");
    let members: Vec<String> = sealed_file(&f, PRODUCT).front.members.into_keys().collect();
    assert_eq!(members, vec![ID1.to_string(), ID2.to_string()]);
}

#[test]
fn preamble_cite_has_no_fragment() {
    let f = two_sections();
    seal_text(
        &f,
        ID1,
        "# S1\n\nIntro holds. [s](../../../../product/demo/section_01.md)\n",
    )
    .expect("seal");
    let cites = sealed_file(&f, ID1).front.cites;
    let keys: Vec<&String> = cites.keys().collect();
    assert_eq!(keys, vec!["doc/product/demo/section_01.md"]);
}

#[test]
fn archived_plan_named_by_metaplan_seals() {
    let meta = metaplan(&["DONE-PLAN-b.md"]);
    let f = fixture(&[
        ("doc/plans/demo/index.md", &meta),
        (
            "doc/plans/demo/archive/DONE-PLAN-b.md",
            "# Plan: B\n\nDoes b.\n",
        ),
    ]);
    let body = "# B\n\nB does b. [p](../../../plans/demo/archive/DONE-PLAN-b.md#plan-b)\n";
    seal_text(&f, "plans/b", body).expect("archived plan sealed");
    assert_eq!(
        sealed_file(&f, "plans/b").front.of,
        "doc/plans/demo/archive/DONE-PLAN-b.md"
    );
}

#[test]
fn seal_removes_draft_and_rejects_foreign_source() {
    let f = two_sections();
    put(&f.root, "doc/product/demo/other.md", "# Other\n");
    let bad = "# S1\n\nOther holds. [s](../../../../product/demo/other.md)\n";
    let err = seal_text(&f, ID1, bad).expect_err("foreign source sealed");
    assert!(
        matches!(&err, SealError::Rejected(e) if e[0].line == 3),
        "{err}"
    );
    assert!(f.paths.drafts.join(format!("{ID1}.md")).exists());
    let good = "# S1\n\nAlpha holds. [s](../../../../product/demo/section_01.md#a)\n";
    seal_text(&f, ID1, good).expect("seal");
    assert!(!f.paths.drafts.join(format!("{ID1}.md")).exists());
}

#[test]
fn unresolved_link_target_is_rejected() {
    let f = two_sections();
    // A URL, a path above the repository root, and a summary link without a sentence id.
    for href in [
        "https://example.com/x",
        "../../../../../../escape.md#a",
        "section_02.md#a",
    ] {
        let draft = format!("# S1\n\nAlpha holds. [s]({href})\n");
        assert_eq!(
            rejections(seal_text(&f, ID1, &draft)),
            [format!(
                "3: link target {href} is neither a source document nor a summary sentence"
            )]
        );
    }
}

#[test]
fn document_summary_cannot_cite_a_summary_sentence() {
    let f = sealed_sections();
    let draft = format!(
        "# S1\n\nAlpha holds. [s](../../../../product/demo/section_01.md#a)\n\n\
         Peer holds. [p](section_02.md#{})\n",
        sentence_id(A2)
    );
    assert_eq!(
        rejections(seal_text(&f, ID1, &draft)),
        ["5: a document summary cites only its source document"]
    );
}

#[test]
fn overview_cannot_cite_a_summary_outside_its_members() {
    let f = sealed_sections();
    put(
        &f.root,
        "doc/product/demo/section_03.md",
        &section(3, "Alpha.", "Beta."),
    );
    seal_document(&f.root, ID3, &[("Gamma holds.", "a")]);
    let links = [
        member(ID1, A1),
        member(ID2, A2),
        member(ID3, "Gamma holds."),
        VISION_LINK.to_string(),
    ];
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        ["7: demo/product/section_03 is not a member of summary overview/demo/product"]
    );
}

#[test]
fn overview_cannot_cite_a_sentence_its_member_lacks() {
    let f = sealed_sections();
    let links = [
        member(ID1, A1),
        member(ID2, A2),
        "[m](../../demo/product/section_01.md#s-00000000)".to_string(),
        VISION_LINK.to_string(),
    ];
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        ["7: demo/product/section_01 has no sentence s-00000000"]
    );
}

#[test]
fn overview_cannot_cite_a_member_whose_summary_is_missing() {
    let f = two_sections();
    seal_document(&f.root, ID1, &[(A1, "a")]);
    let links = [member(ID1, A1), member(ID2, A2), VISION_LINK.to_string()];
    // The missing member is also uncited: only a current sentence counts as a citation.
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        [
            "1: member summary demo/product/section_02 is not cited".to_string(),
            format!(
                "5: demo/product/section_02 has no sentence {}",
                sentence_id(A2)
            ),
        ]
    );
}

#[test]
fn overview_cannot_cite_a_stale_member_sentence() {
    let f = sealed_sections();
    put(
        &f.root,
        "doc/product/demo/section_01.md",
        &section(1, "Alpha moved.", "Beta."),
    );
    let links = [member(ID1, A1), member(ID2, A2), VISION_LINK.to_string()];
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        [
            "1: member summary demo/product/section_01 is not cited".to_string(),
            format!(
                "3: sentence {} of demo/product/section_01 is stale",
                sentence_id(A1)
            ),
        ]
    );
}

#[test]
fn overview_cannot_cite_a_raw_passage_outside_its_sources() {
    let f = sealed_sections();
    let links = [
        member(ID1, A1),
        member(ID2, A2),
        "[s](../../../../product/demo/section_01.md#a)".to_string(),
        VISION_LINK.to_string(),
    ];
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        ["7: link target doc/product/demo/section_01.md is not a source of summary overview/demo/product"]
    );
    // Engineering takes its passages from the metaplans, not from the plans themselves.
    let f = sealed_plan();
    let links = [
        member("plans/b", "B does b."),
        METAPLAN_LINK.to_string(),
        "[p](../../../../plans/demo/PLAN-b.md#plan-b)".to_string(),
    ];
    assert_eq!(
        rejections(seal_text(&f, ENGINEERING, &overview_draft(&links))),
        ["7: link target doc/plans/demo/PLAN-b.md is not a source of summary overview/demo/engineering"]
    );
}

#[test]
fn overview_must_cite_every_member() {
    let f = sealed_sections();
    let links = [member(ID1, A1), VISION_LINK.to_string()];
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        ["1: member summary demo/product/section_02 is not cited"]
    );
}

#[test]
fn overview_must_cite_its_source_documents() {
    let f = sealed_sections();
    let links = [member(ID1, A1), member(ID2, A2)];
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        ["1: source doc/product/demo/index.md is not cited"]
    );
    let f = sealed_plan();
    let links = [member("plans/b", "B does b.")];
    assert_eq!(
        rejections(seal_text(&f, ENGINEERING, &overview_draft(&links))),
        ["1: source doc/plans/demo/index.md is not cited"]
    );
}

#[test]
fn cross_feature_summary_link_resolves_and_is_rejected_outside_scope() {
    let f = sealed_sections();
    let other = "other/product/section_01";
    put(
        &f.root,
        "doc/product/other/section_01.md",
        &section(1, "Other.", "Beta."),
    );
    seal_document(&f.root, other, &[("Other holds.", "a")]);
    let target = summary_path(other);
    let href = link_from_summary(ID1, &target);
    assert_eq!(href, "../../other/product/section_01.md");
    let sid = sentence_id("Other holds.");
    assert_eq!(
        resolve_link(ID1, &format!("{href}#{sid}")),
        Some((target, Some(sid)))
    );
    let links = [
        member(ID1, A1),
        member(ID2, A2),
        member(other, "Other holds."),
        VISION_LINK.to_string(),
    ];
    assert_eq!(
        rejections(seal_text(&f, PRODUCT, &overview_draft(&links))),
        ["7: other/product/section_01 is not a member of summary overview/demo/product"]
    );
}
