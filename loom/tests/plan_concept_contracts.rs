//! Contracts for the concept page data: key chips and colours, overview citations and
//! marks, plan implements links, unlisted plans, read-only out-of-date reasons and
//! page key parsing.

#[path = "plan_concept_contracts/support.rs"]
mod support;

use loom::plan_server::concept::{PageContext, PageKey, Scope};
use serde_json::{json, Value};
use support::*;

/// A product project listing `numbers` in vision order, each section sealed with one
/// sentence, and a product overview citing every member and the vision.
fn product_project(numbers: &[u32]) -> (tempfile::TempDir, std::path::PathBuf) {
    let mut files = vec![(VISION.to_string(), vision(numbers))];
    for n in numbers {
        files.push((section_path(*n), section(*n, &["Rules"])));
    }
    let (temp, root, home) = tree(&files);
    let mut overview = "# Product\n\n".to_string();
    for n in numbers {
        let text = format!("Section {n} holds.");
        sealed(
            &root,
            &home,
            &section_id(*n),
            &section_draft(*n, &[(&text, "rules")]),
        );
        let link = member_link(&section_id(*n), &text);
        overview.push_str(&format!("Member {n} is kept. [§]({link})\n\n"));
    }
    overview.push_str(&format!("The vision frames them. [§]({VISION_LINK})\n"));
    sealed(&root, &home, "product", &overview);
    (temp, root)
}

#[test]
fn key_colours_repeat_after_eight() {
    let order = [3, 1, 2, 4, 5, 6, 7, 8, 9];
    let (_t, root) = product_project(&order);
    let page = page_of(&root, &product_overview_key());
    let paths = doc_paths(&page, "key_chips", "doc");
    let expected: Vec<String> = order.iter().map(|n| section_path(*n)).collect();
    assert_eq!(paths, expected, "chips follow the vision table order");
    let colours: Vec<Value> = page["key_chips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["colour"].clone())
        .collect();
    let want: Vec<Value> = [1, 2, 3, 4, 5, 6, 7, 8, 1]
        .iter()
        .map(|c| json!(c))
        .collect();
    assert_eq!(colours, want, "colour = index % 8 + 1 in vision order");
}

#[test]
fn overview_rests_on_cited_sources() {
    let files = [
        (VISION.to_string(), vision(&[1])),
        (
            section_path(1),
            section(1, &["Detection rules", "Pressure tests"]),
        ),
    ];
    let (_t, root, home) = tree(&files);
    let (a, b) = ("Detection is rule based.", "Pressure tests probe plans.");
    let draft = section_draft(1, &[(a, "detection-rules"), (b, "pressure-tests")]);
    sealed(&root, &home, &section_id(1), &draft);
    let (la, lb) = (
        member_link(&section_id(1), a),
        member_link(&section_id(1), b),
    );
    let overview = format!(
        "# Product\n\nDetection is tested. [§]({la}) [§]({lb})\n\n\
         The vision frames it. [§]({VISION_LINK})\n"
    );
    sealed(&root, &home, "product", &overview);
    let page = page_of(&root, &product_overview_key());
    let s = sentence(&page, "Detection is tested.");
    let citations = json!([
        {"kind": "sentence", "summary": "product/section_01", "id": sid(a)},
        {"kind": "sentence", "summary": "product/section_01", "id": sid(b)},
    ]);
    assert_eq!(
        s["citations"], citations,
        "summary-sentence citations in link order"
    );
    let rests_on = json!([
        {"path": "doc/product/section_01.md", "anchor": "detection-rules"},
        {"path": "doc/product/section_01.md", "anchor": "pressure-tests"},
    ]);
    assert_eq!(
        s["rests_on"], rests_on,
        "the cited sentences' source anchors"
    );
    let marks = s["marks"].as_array().expect("marks");
    assert_eq!(
        marks.len(),
        1,
        "one mark per distinct cited document: {marks:?}"
    );
    assert_eq!(marks[0]["doc"]["path"], "doc/product/section_01.md");
}

#[test]
fn vision_citation_is_source() {
    let (_t, root) = product_project(&[1]);
    let page = page_of(&root, &product_overview_key());
    let s = sentence(&page, "The vision frames them.");
    let citations = json!([{"kind": "source", "path": "doc/product/index.md", "anchor": "vision"}]);
    assert_eq!(
        s["citations"], citations,
        "a vision link is Citation::Source"
    );
    let rests_on = json!([{"path": "doc/product/index.md", "anchor": "vision"}]);
    assert_eq!(
        s["rests_on"], rests_on,
        "a direct source citation is rested on"
    );
    let marks = s["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1, "one mark for the vision: {marks:?}");
    assert_eq!(marks[0]["colour"], 0, "the vision's mark has colour 0");
    assert_eq!(marks[0]["doc"]["path"], "doc/product/index.md");
}

#[test]
fn implements_not_stated() {
    let (path, text) = plan("a", "");
    let (_t, root, home) = tree(&[(path, text)]);
    sealed(&root, &home, "plans/a", &plan_draft("a"));
    let page = page_of(&root, &summary_key(Scope::Engineering, "a"));
    assert_eq!(page["path"], "doc/plans/PLAN-a.md");
    assert_eq!(
        page["implements"],
        json!({"kind": "not_stated"}),
        "a plan with no Implements: line"
    );
}

#[test]
fn implements_broken_name_kept() {
    let (path, text) = plan("a", "Implements: section_02, section_99\n\n");
    let files = [
        (VISION.to_string(), vision(&[2])),
        (section_path(2), section(2, &["Rules"])),
        (path, text),
    ];
    let (_t, root, home) = tree(&files);
    sealed(&root, &home, "plans/a", &plan_draft("a"));
    let page = page_of(&root, &summary_key(Scope::Engineering, "a"));
    let implements = &page["implements"];
    assert_eq!(implements["kind"], "sections", "{implements}");
    let links = implements["links"].as_array().expect("links");
    assert_eq!(links.len(), 2, "both named sections are kept: {links:?}");
    assert_eq!(links[0]["number"], 2);
    assert_eq!(links[0]["doc"]["path"], "doc/product/section_02.md");
    assert_eq!(links[1]["number"], 99);
    assert_eq!(links[1]["name"], "section_99", "the broken name as written");
    assert_eq!(
        links[1]["doc"],
        Value::Null,
        "a broken link has no document"
    );
}

#[test]
fn unlisted_plans_listed() {
    let (pa, ta) = plan("a", "");
    let (pb, tb) = plan("b", "");
    let files = [
        (META.to_string(), metaplan(&["PLAN-a.md"])),
        (pa, ta),
        (pb, tb),
    ];
    let (_t, root, home) = tree(&files);
    sealed(&root, &home, "plans/a", &plan_draft("a"));
    sealed(&root, &home, "plans/b", &plan_draft("b"));
    let link = member_link("plans/a", &plan_sentence("a"));
    let overview = format!(
        "# Engineering\n\nA is planned. [§]({link})\n\n\
         The metaplan orders it. [§]({META_LINK})\n"
    );
    sealed(&root, &home, "engineering", &overview);
    let page = page_of(&root, &engineering_overview_key());
    let chips = doc_paths(&page, "key_chips", "doc");
    assert_eq!(
        chips,
        vec!["doc/plans/PLAN-a.md"],
        "only metaplan plans get chips"
    );
    let unlisted: Vec<&str> = page["unlisted"]
        .as_array()
        .expect("unlisted array")
        .iter()
        .filter_map(|d| d["path"].as_str())
        .collect();
    assert_eq!(unlisted, vec!["doc/plans/PLAN-b.md"], "plan b is unlisted");
}

#[test]
fn read_only_stale_reason() {
    let (_t, root) = product_project(&[1]);
    put(
        &root,
        &section_path(1),
        &section(1, &["Rules"]).replace("Rules text.", "Edited."),
    );
    let key = summary_key(Scope::Product, "section_01");
    let writable = page_of(&root, &key);
    let reason = &writable["state"]["out_of_date"]["reason"];
    assert_eq!(reason, "source_changed", "{}", writable["state"]);
    let ctx = PageContext {
        read_only: Some("tmux is not installed".into()),
        ..Default::default()
    };
    let read_only = page(&root, &key, &ctx);
    let out_of_date = &read_only["state"]["out_of_date"];
    assert_eq!(out_of_date["reason"], "cannot_rebuild", "{out_of_date}");
    assert_eq!(out_of_date["detail"], "tmux is not installed");
}

#[test]
fn rejects_unsafe_page_key() {
    for rest in [
        "product/../x",
        "product/a/b",
        "product/",
        "engineering/%2e%2e",
    ] {
        assert!(PageKey::parse(rest).is_none(), "{rest} must be rejected");
    }
    let parsed = PageKey::parse("product/section_04");
    assert!(
        matches!(
            &parsed,
            Some(PageKey::Summary { scope: Scope::Product, doc }) if doc == "section_04"
        ),
        "product/section_04 is a product summary key"
    );
}
