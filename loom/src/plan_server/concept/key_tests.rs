//! Tests for key chips.

use std::path::Path;

use super::body::doc_link;
use super::fixtures;
use super::key::*;
use super::page::{PageContext, Scope};
use crate::planning::summary::evaluate::evaluate;

#[test]
fn chips_follow_scope_order() {
    let fixture = fixtures::fixture(&[
        (fixtures::VISION, &fixtures::vision(&[2, 1])),
        (fixtures::S1, "# First\n"),
        (fixtures::S2, "# Second\n"),
        (
            fixtures::META,
            "# Plans\n\n## One (`PLAN-b.md`)\n\n## Two (`PLAN-a.md`)\n",
        ),
        ("doc/plans/demo/PLAN-a.md", "# A\n"),
        ("doc/plans/demo/DONE-PLAN-b.md", "# B\n"),
    ]);
    let project = fixtures::load(&fixture.root);
    let eval = evaluate(&project, &[]);
    let ctx = PageContext::default();
    let product = key_chips(&project, &eval, "demo", Scope::Product, &ctx);
    let engineering = key_chips(&project, &eval, "demo", Scope::Engineering, &ctx);
    assert_eq!(
        product
            .iter()
            .map(|c| c.doc.id.as_str())
            .collect::<Vec<_>>(),
        ["demo/product/section_02", "demo/product/section_01"]
    );
    assert_eq!(
        engineering
            .iter()
            .map(|c| c.doc.id.as_str())
            .collect::<Vec<_>>(),
        ["plans/b", "plans/a"]
    );
    assert_eq!(engineering[0].doc.path, "doc/plans/demo/DONE-PLAN-b.md");
    assert_eq!(engineering[0].doc.page, "demo/engineering/b");
}

#[test]
fn colours_repeat_after_eight() {
    let fixture = fixtures::two_sections();
    for n in 3..=9 {
        fixtures::put(
            &fixture.root,
            &format!("doc/product/demo/section_{n:02}.md"),
            &fixtures::section(n, "A", "B"),
        );
    }
    fixtures::put(
        &fixture.root,
        fixtures::VISION,
        &fixtures::vision(&[1, 2, 3, 4, 5, 6, 7, 8, 9]),
    );
    let project = fixtures::load(&fixture.root);
    let chips = key_chips(
        &project,
        &evaluate(&project, &[]),
        "demo",
        Scope::Product,
        &PageContext::default(),
    );
    assert_eq!(
        chips.iter().map(|c| c.colour).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6, 7, 8, 1]
    );
}

#[test]
fn colours_follow_the_chips_that_remain() {
    let long = "a".repeat(129);
    let long_entry = format!("PLAN-{long}.md");
    let long_file = format!("doc/plans/demo/{long_entry}");
    let fixture = fixtures::fixture(&[
        (
            fixtures::META,
            &fixtures::metaplan(&[long_entry.as_str(), "PLAN-a.md", "PLAN-b.md"]),
        ),
        (&long_file, "# Long\n"),
        ("doc/plans/demo/PLAN-a.md", "# A\n"),
        ("doc/plans/demo/PLAN-b.md", "# B\n"),
    ]);
    let project = fixtures::load(&fixture.root);
    let chips = key_chips(
        &project,
        &evaluate(&project, &[]),
        "demo",
        Scope::Engineering,
        &PageContext::default(),
    );
    let ids: Vec<_> = chips.iter().map(|chip| chip.doc.id.as_str()).collect();
    assert_eq!(ids, ["plans/a", "plans/b"]);
    assert_eq!(chips.iter().map(|c| c.colour).collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn no_plan_only_with_an_engineering_scope() {
    let fixture = fixtures::two_sections();
    fixtures::put(
        &fixture.root,
        "doc/plans/demo/PLAN-a.md",
        "# A\n\nImplements: section_01\n",
    );
    let project = fixtures::load(&fixture.root);
    let ctx = PageContext::default();
    let chips = key_chips(
        &project,
        &evaluate(&project, &[]),
        "demo",
        Scope::Product,
        &ctx,
    );
    assert!(chips.iter().all(|c| !c.no_plan));
    fixtures::put(
        &fixture.root,
        fixtures::META,
        "# Plans\n\n## A (`PLAN-a.md`)\n",
    );
    let project = fixtures::load(&fixture.root);
    let eval = evaluate(&project, &[]);
    let chips = key_chips(&project, &eval, "demo", Scope::Product, &ctx);
    assert_eq!(
        chips.iter().map(|c| c.no_plan).collect::<Vec<_>>(),
        [false, true]
    );
    assert!(!key_chips(&project, &eval, "demo", Scope::Engineering, &ctx)[0].no_plan);
}

#[test]
fn chip_carries_colour_label_and_title() {
    let fixture = fixtures::two_sections();
    let project = fixtures::load(&fixture.root);
    let mut eval = evaluate(&project, &[]);
    eval.summaries.remove(fixtures::ID1);
    let chips = key_chips(
        &project,
        &eval,
        "demo",
        Scope::Product,
        &PageContext::default(),
    );
    assert_eq!(
        (
            &chips[0].doc.id,
            &chips[0].doc.title,
            &chips[0].doc.path,
            &chips[0].doc.page,
            &chips[0].label,
            chips[0].colour
        ),
        (
            &"demo/product/section_01".to_string(),
            &"Section 01".to_string(),
            &"doc/product/demo/section_01.md".to_string(),
            &"demo/product/section_01".to_string(),
            &"01".to_string(),
            1
        )
    );
    assert_eq!(chips[0].out_of_date, None);
    assert_eq!(colour_of(&chips, "doc/product/demo/section_02.md"), Some(2));
    assert_eq!(colour_of(&chips, "doc/missing.md"), None);
}

#[test]
fn a_plan_whose_page_key_is_too_long_has_no_chip_and_no_link() {
    let long = "a".repeat(129);
    let long_file = format!("doc/plans/demo/PLAN-{long}.md");
    let fixture = fixtures::fixture(&[
        (
            fixtures::META,
            &fixtures::metaplan(&["PLAN-a.md", &format!("PLAN-{long}.md")]),
        ),
        ("doc/plans/demo/PLAN-a.md", "# A\n"),
        (&long_file, "# Long\n"),
    ]);
    let project = fixtures::load(&fixture.root);
    let eval = evaluate(&project, &[]);
    assert!(crate::planning::summary::store::scope(&project, "demo")
        .engineering
        .contains(&format!("plans/{long}")));
    let chips = key_chips(
        &project,
        &eval,
        "demo",
        Scope::Engineering,
        &PageContext::default(),
    );
    let ids: Vec<_> = chips.iter().map(|chip| chip.doc.id.as_str()).collect();
    assert_eq!(ids, ["plans/a"]);
    assert!(doc_link(&project, Path::new(&long_file)).is_none());
    let short = doc_link(&project, Path::new("doc/plans/demo/PLAN-a.md")).expect("a link");
    assert_eq!(short.page, "demo/engineering/a");
}
