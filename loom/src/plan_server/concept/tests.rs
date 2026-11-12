//! Tests for page loading: chips, empty states, bodies, marks and plan fields.

use super::fixtures::{self as fx, Fixture};
use super::page::{load_page, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{Block, Citation, ConceptPage, EmptyKind, Sentence};
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::format::{self, sentence_id};
use crate::planning::summary::store::summary_path;

fn load(f: &Fixture, key: &PageKey, ctx: &PageContext) -> ConceptPage {
    let project = fx::load(&f.root);
    let eval = evaluate(&project, &[]);
    load_page(&project, &eval, key, ctx).expect("the page exists")
}

fn page(f: &Fixture, key: &PageKey) -> ConceptPage {
    load(f, key, &PageContext::default())
}

fn product() -> PageKey {
    PageKey::Overview {
        feature: "demo".into(),
    }
}

fn summary(scope: Scope, doc: &str) -> PageKey {
    PageKey::Summary {
        feature: Some("demo".into()),
        scope,
        doc: doc.to_owned(),
    }
}

fn plan(slug: &str, extra: &str) -> (String, String) {
    (
        format!("doc/plans/demo/PLAN-{slug}.md"),
        format!("# Plan: {slug}\n\n{extra}Does {slug}.\n"),
    )
}

fn sentences(page: &ConceptPage) -> Vec<&Sentence> {
    let paragraphs = page.blocks.iter().filter_map(|block| match block {
        Block::Paragraph { sentences } => Some(sentences),
        Block::Heading { .. } => None,
    });
    paragraphs.flatten().collect()
}

/// Sections 1 and 2 under a vision, each sealed with `Section n holds.`
fn sealed_sections() -> Fixture {
    let f = fx::two_sections();
    fx::seal_document(&f.root, fx::ID1, &[("Section 1 holds.", "a")]);
    fx::seal_document(&f.root, fx::ID2, &[("Section 2 holds.", "a")]);
    f
}

#[test]
fn follow_up_entries_give_chips_after_the_numbered_entries() {
    let (pa, ta) = plan("a", "");
    let (pb, tb) = plan("b", "");
    let (pc, tc) = plan("c", "");
    let index = format!(
        "{}## Follow-ups\n\n## 2. E2 (`PLAN-a.md`)\n\n## 3. E3 (`PLAN-c.md`)\n",
        fx::metaplan(&["PLAN-b.md"])
    );
    let f = fx::fixture(&[(fx::META, &index), (&pa, &ta), (&pb, &tb), (&pc, &tc)]);
    let chips = page(&f, &summary(Scope::Metaplan, "index")).key_chips;
    let slugs: Vec<_> = chips.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(slugs, ["b", "a", "c"]);
    let colours: Vec<_> = chips.iter().map(|c| c.colour).collect();
    assert_eq!(colours, [1, 2, 3]);
}

#[test]
fn latest_pressure_date_wins() {
    let extra = "## Pressure tests\n\n- 2026-03-04: second\n- 2026-01-02: first\n\n";
    let (path, text) = plan("a", extra);
    let f = fx::fixture(&[(fx::META, &fx::metaplan(&["PLAN-a.md"])), (&path, &text)]);
    fx::seal_document(&f.root, "plans/a", &[("Plan a does.", "plan-a")]);
    let page = page(&f, &summary(Scope::Engineering, "a"));
    assert_eq!(page.last_pressure.as_deref(), Some("2026-03-04"));
    assert_eq!(page.plan_state.as_deref(), Some("written"));
    assert_eq!(page.path.as_deref(), Some("doc/plans/demo/PLAN-a.md"));
}

#[test]
fn summary_page_colour_equals_its_overview_chip_colour() {
    let f = sealed_sections();
    let overview = page(&f, &product());
    let chip = &overview.key_chips[1];
    let one = page(&f, &summary(Scope::Product, "section_02"));
    assert_eq!(one.colour, Some(chip.colour));
    assert_eq!(chip.doc.path, "doc/product/demo/section_02.md");
    assert!(
        one.file_hash.is_some(),
        "a summary with a file carries its hash"
    );
}

#[test]
fn sentence_without_links_has_no_marks() {
    let f = fx::two_sections();
    let mut file = fx::seal_document(&f.root, fx::ID1, &[("Linked.", "a")]);
    for block in &mut file.blocks {
        if let format::Block::Paragraph(sentences) = block {
            for link in sentences.iter_mut().flat_map(|one| one.links.iter_mut()) {
                link.target = "https://example.com/x".to_owned();
                link.resolved = format::LinkTarget::Unresolved(link.target.clone());
            }
        }
    }
    fx::write_file(&f.root, fx::ID1, &file);
    let page = page(&f, &summary(Scope::Product, "section_01"));
    let all = sentences(&page);
    assert_eq!(all.len(), 1);
    assert!(all[0].marks.is_empty() && all[0].citations.is_empty() && all[0].rests_on.is_empty());
}

#[test]
fn implemented_by_on_a_section_page() {
    let (path, text) = plan("a", "Implements: section_01\n\n");
    let f = fx::fixture(&[
        (fx::VISION, &fx::vision(&[1])),
        (fx::S1, &fx::section(1, "Alpha.", "Beta.")),
        (fx::META, &fx::metaplan(&["PLAN-a.md"])),
        (&path, &text),
    ]);
    let page = page(&f, &summary(Scope::Product, "section_01"));
    let slugs: Vec<_> = page
        .implemented_by
        .iter()
        .map(|p| p.slug.as_str())
        .collect();
    assert_eq!(slugs, ["a"]);
    assert_eq!(page.implemented_by[0].state, "written");
}

#[test]
fn plans_on_the_first_sentence_of_each_paragraph_only() {
    let (path, text) = plan("a", "Implements: section_01\n\n");
    let f = fx::fixture(&[
        (fx::VISION, &fx::vision(&[1])),
        (fx::S1, &fx::section(1, "Alpha.", "Beta.")),
        (fx::META, &fx::metaplan(&["PLAN-a.md"])),
        (&path, &text),
    ]);
    let lines = [
        ("One.", "a"),
        ("Two.", "a"),
        ("Three.", "b"),
        ("Four.", "b"),
    ];
    let mut file = fx::seal_document(&f.root, fx::ID1, &lines);
    let all: Vec<format::Sentence> = file
        .blocks
        .iter()
        .flat_map(|block| match block {
            format::Block::Paragraph(s) => s.clone(),
            format::Block::Heading { .. } => Vec::new(),
        })
        .collect();
    file.blocks = all
        .chunks(2)
        .map(|c| format::Block::Paragraph(c.to_vec()))
        .collect();
    fx::write_file(&f.root, fx::ID1, &file);
    let page = page(&f, &summary(Scope::Product, "section_01"));
    let with_plans: Vec<_> = sentences(&page)
        .iter()
        .map(|s| !s.plans.is_empty())
        .collect();
    assert_eq!(with_plans, [true, false, true, false]);
}

#[test]
fn sections_no_vision_lists_in_file_name_order() {
    let (s2, s1) = (fx::section(2, "A.", "B."), fx::section(1, "A.", "B."));
    let f = fx::fixture(&[(fx::S2, &s2), (fx::S1, &s1)]);
    let empty = page(&f, &product()).empty.expect("empty state");
    assert_eq!(empty.kind, EmptyKind::SectionsNoVision);
    let paths: Vec<_> = empty.docs.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, [fx::S1, fx::S2]);
}

#[test]
fn no_vision_offers_a_start() {
    let f = fx::fixture(&[("doc/product/demo/notes.md", "# Notes\n")]);
    let vision = page(&f, &product()).empty.expect("empty state");
    assert_eq!(vision.kind, EmptyKind::NoVision);
    assert_eq!(vision.offer.as_deref(), Some("start-vision"));
}

#[test]
fn never_built_counts_the_documents() {
    let f = fx::two_sections();
    let empty = page(&f, &product()).empty.expect("empty state");
    assert_eq!(empty.kind, EmptyKind::NeverBuilt);
    assert_eq!(empty.count, Some(2));
}

#[test]
fn vision_without_sections_is_not_never_built() {
    let f = fx::fixture(&[(fx::VISION, &fx::vision(&[]))]);
    let empty = page(&f, &product()).empty.expect("empty state");
    assert_eq!(empty.kind, EmptyKind::Building);
    assert_eq!(empty.count, None);
}

#[test]
fn building_state_keeps_the_key() {
    let f = fx::two_sections();
    fx::seal_document(&f.root, fx::ID1, &[("Section 1 holds.", "a")]);
    let page = page(&f, &product());
    assert_eq!(page.empty.expect("empty state").kind, EmptyKind::Building);
    assert_eq!(page.key_chips.len(), 2);
}

#[test]
fn sealed_overview_renders_whatever_its_state_and_missing_gives_building() {
    let f = sealed_sections();
    fx::seal_overview(
        &f.root,
        "overview/demo/product",
        &[(fx::ID1, "Section 1 holds.")],
        &[],
    );
    fx::put(&f.root, fx::S1, &fx::section(1, "Edited.", "Beta."));
    let stale = page(&f, &product());
    assert!(stale.empty.is_none() && !stale.blocks.is_empty());
    assert!(stale.file_hash.is_some());
    std::fs::remove_file(f.root.join(summary_path("overview/demo/product")))
        .expect("remove overview");
    let missing = page(&f, &product());
    assert_eq!(
        missing.empty.expect("empty state").kind,
        EmptyKind::Building
    );
    assert!(missing.blocks.is_empty());
}

#[test]
fn missing_cited_sentence_keeps_the_citation_and_dashes_its_mark() {
    let f = fx::two_sections();
    fx::seal_document(&f.root, fx::ID1, &[("Old claim.", "a")]);
    fx::seal_overview(
        &f.root,
        "overview/demo/product",
        &[(fx::ID1, "Old claim.")],
        &[],
    );
    fx::seal_document(&f.root, fx::ID2, &[("Other.", "a")]);
    fx::seal_document(&f.root, fx::ID1, &[("New claim.", "a")]);
    let page = page(&f, &product());
    let cited = sentences(&page)[0];
    let want = Citation::Sentence {
        summary: fx::ID1.to_owned(),
        id: sentence_id("Old claim."),
    };
    assert_eq!(cited.citations, [want]);
    assert!(
        cited.rests_on.is_empty(),
        "a missing sentence rests on nothing"
    );
    assert_eq!(cited.marks.len(), 1);
    assert!(cited.marks[0].out_of_date.is_some(), "the mark is dashed");
}

#[test]
fn page_keys_round_trip_and_reject_unsafe_names() {
    for key in [
        "overview/demo/product",
        "demo/metaplan/index",
        "demo/product/section_04",
        "demo/engineering/my-plan",
        "plans/standalone",
        "overview/another-feature/product",
    ] {
        assert_eq!(PageKey::parse(key).expect(key).as_key(), key);
    }
    assert!(PageKey::parse("other/x").is_none());
}

#[test]
fn page_key_name_rule() {
    assert_eq!(
        PageKey::parse("demo/engineering/feat.v2"),
        Some(summary(Scope::Engineering, "feat.v2"))
    );
    let longest = "a".repeat(128);
    assert_eq!(
        PageKey::parse(&format!("demo/product/{longest}")),
        Some(summary(Scope::Product, &longest))
    );
    let too_long = "a".repeat(129);
    assert!(PageKey::parse(&format!("demo/product/{too_long}")).is_none());
    for key in [
        "demo/product/.",
        "demo/product/..",
        "demo/product/a%2e",
        "demo/product/",
        "demo/product/a/b",
        "other",
        "product",
        "engineering",
        "overview/Bad/product",
        "overview/../product",
        "overview/demo/other",
    ] {
        assert!(PageKey::parse(key).is_none(), "{key} must be rejected");
    }
}
