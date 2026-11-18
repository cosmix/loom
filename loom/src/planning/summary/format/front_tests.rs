//! Tests for front matter.

use std::collections::BTreeMap;

use super::front::{parse_front, render_front, FrontMatter, SummaryKind};

const OVERVIEW: &str = "overview/alpha/product";
const SECTION: &str = "alpha/product/section_01";
const PLAN: &str = "plans/x";

/// The YAML between the `---` lines of rendered front matter.
fn yaml_of(rendered: &str) -> &str {
    rendered
        .strip_prefix("---\n")
        .and_then(|rest| rest.strip_suffix("---\n"))
        .unwrap_or_else(|| panic!("not front matter:\n{rendered}"))
}

#[test]
fn maps_round_trip_special_characters_and_render_stably() {
    let front = FrontMatter {
        kind: SummaryKind::Overview,
        of: OVERVIEW.into(),
        file: None,
        bundle: BTreeMap::new(),
        cites: BTreeMap::from([
            (
                "doc/a.md#heading: \"quote\"".into(),
                "sha256:hash\\value".into(),
            ),
            ("doc/z.md".into(), "sha256:other".into()),
        ]),
        members: BTreeMap::from([(
            "alpha/product/section_01#x: \"y\"".into(),
            "hash\nvalue".into(),
        )]),
    };
    let rendered = render_front(&front);
    assert_eq!(rendered, render_front(&front));
    assert_eq!(parse_front(yaml_of(&rendered), 2, OVERVIEW).unwrap(), front);
}

fn plan_front(bundle: BTreeMap<String, String>) -> FrontMatter {
    FrontMatter {
        kind: SummaryKind::Document,
        of: "doc/plans/demo/PLAN-x.md".into(),
        file: Some("sha256:plan".into()),
        bundle,
        cites: BTreeMap::from([("doc/plans/demo/PLAN-x.md#risks".into(), "sha256:r".into())]),
        members: BTreeMap::new(),
    }
}

#[test]
fn a_plan_bundle_round_trips_and_an_empty_one_is_not_written() {
    let bundle = BTreeMap::from([
        (
            "doc/plans/demo/briefs/x/build/w1.md".into(),
            "sha256:w1".into(),
        ),
        (
            "doc/plans/demo/briefs/x/common.md".into(),
            "sha256:c".into(),
        ),
    ]);
    let with = plan_front(bundle);
    let rendered = render_front(&with);
    assert!(rendered.contains("\nbundle:\n"), "{rendered}");
    assert_eq!(parse_front(yaml_of(&rendered), 2, PLAN).unwrap(), with);

    let without = plan_front(BTreeMap::new());
    let rendered = render_front(&without);
    assert!(!rendered.contains("bundle"), "{rendered}");
    assert_eq!(parse_front(yaml_of(&rendered), 2, PLAN).unwrap(), without);
}

#[test]
fn bundle_is_refused_on_an_overview_and_on_a_non_plan_document() {
    let bundle = "bundle:\n  \"doc/plans/demo/briefs/x/common.md\": \"sha256:c\"\n";
    let overview = format!("kind: overview\nof: {OVERVIEW}\n{bundle}cites: {{}}\nmembers: {{}}\n");
    let errors = parse_front(&overview, 2, OVERVIEW).unwrap_err();
    assert!(
        errors[0]
            .message
            .contains("overview front matter must not contain bundle"),
        "{errors:?}"
    );
    let document = format!("kind: document\nof: doc/a.md\nfile: sha256:abc\n{bundle}cites: {{}}\n");
    let errors = parse_front(&document, 2, SECTION).unwrap_err();
    assert!(
        errors[0].message.contains("only a plan summary"),
        "{errors:?}"
    );
    assert!(parse_front(&document, 2, PLAN).is_ok());
}

#[test]
fn unknown_key_reports_its_line() {
    let yaml = "kind: document\nof: doc/a.md\nfile: sha256:abc\ncites: {}\nsurprise: value\n";
    let errors = parse_front(yaml, 2, SECTION).unwrap_err();
    assert_eq!(errors[0].line, 6);
    assert!(errors[0].message.contains("surprise"));
}

#[test]
fn document_requires_file() {
    let yaml = "kind: document\nof: doc/a.md\ncites: {}\n";
    assert!(parse_front(yaml, 2, SECTION).unwrap_err()[0]
        .message
        .contains("file"));
}

#[test]
fn overview_requires_members_and_document_cannot_have_them() {
    let overview = format!("kind: overview\nof: {OVERVIEW}\ncites: {{}}\n");
    assert!(parse_front(&overview, 2, OVERVIEW).is_err());
    assert!(parse_front(
        "kind: document\nof: doc/a.md\nfile: sha256:abc\ncites: {}\nmembers: {}\n",
        2,
        SECTION
    )
    .is_err());
}

#[test]
fn overview_front_matter_requires_a_feature_overview_id() {
    for id in ["product", "engineering", "overview/Bad/product", "plans/p"] {
        let yaml = format!("kind: overview\nof: {id}\ncites: {{}}\nmembers: {{}}\n");
        let errors = parse_front(&yaml, 2, OVERVIEW).unwrap_err();
        assert!(
            errors[0].message.contains("valid feature overview id"),
            "{id}"
        );
    }
}
