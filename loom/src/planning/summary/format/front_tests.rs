//! Tests for front matter.

use std::collections::BTreeMap;

use super::front::{parse_front, render_front, FrontMatter, SummaryKind};

#[test]
fn maps_round_trip_special_characters_and_render_stably() {
    let front = FrontMatter {
        kind: SummaryKind::Overview,
        of: "product".into(),
        file: None,
        cites: BTreeMap::from([
            (
                "doc/a.md#heading: \"quote\"".into(),
                "sha256:hash\\value".into(),
            ),
            ("doc/z.md".into(), "sha256:other".into()),
        ]),
        members: BTreeMap::from([("product/section_01#x: \"y\"".into(), "hash\nvalue".into())]),
    };
    let rendered = render_front(&front);
    assert_eq!(rendered, render_front(&front));
    let yaml = rendered
        .strip_prefix("---\n")
        .unwrap()
        .strip_suffix("---\n")
        .unwrap();
    assert_eq!(parse_front(yaml, 2).unwrap(), front);
}

#[test]
fn unknown_key_reports_its_line() {
    let yaml = "kind: document\nof: doc/a.md\nfile: sha256:abc\ncites: {}\nsurprise: value\n";
    let errors = parse_front(yaml, 2).unwrap_err();
    assert_eq!(errors[0].line, 6);
    assert!(errors[0].message.contains("surprise"));
}

#[test]
fn document_requires_file() {
    let yaml = "kind: document\nof: doc/a.md\ncites: {}\n";
    assert!(parse_front(yaml, 2).unwrap_err()[0]
        .message
        .contains("file"));
}

#[test]
fn overview_requires_members_and_document_cannot_have_them() {
    assert!(parse_front("kind: overview\nof: product\ncites: {}\n", 2).is_err());
    assert!(parse_front(
        "kind: document\nof: doc/a.md\nfile: sha256:abc\ncites: {}\nmembers: {}\n",
        2
    )
    .is_err());
}
