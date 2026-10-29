//! Feature summary ids through parsing, rendering and link classification.

use std::collections::BTreeMap;

use super::*;

#[test]
fn feature_overviews_round_trip_with_their_member_ids() {
    let cases = [
        (
            "overview/alpha/product",
            "alpha/product/section_01",
            "../../alpha/product/section_01.md#s-1234abcd",
        ),
        (
            "overview/beta/engineering",
            "plans/p",
            "../../plans/p.md#s-1234abcd",
        ),
    ];
    for (id, member, href) in cases {
        let body = parse_draft(id, &format!("# Overview\n\nOne. [member]({href})\n")).unwrap();
        let original = SummaryFile {
            front: FrontMatter {
                kind: SummaryKind::Overview,
                of: id.into(),
                file: None,
                cites: BTreeMap::new(),
                members: BTreeMap::from([(member.into(), "sha256:member".into())]),
            },
            title: body.title,
            blocks: body.blocks,
        };
        let text = render(&original);
        assert!(text.contains(&format!("of: {id}\n")));
        let parsed = parse(&store::summary_path(id), &text).unwrap();
        assert_eq!(parsed.front, original.front);
        assert_eq!(parsed.title, original.title);
        assert_eq!(render(&parsed), text);
        let Block::Paragraph(sentences) = &parsed.blocks[0] else {
            panic!("expected a paragraph");
        };
        assert_eq!(
            sentences[0].links[0].resolved,
            LinkTarget::Sentence {
                summary: member.into(),
                id: "s-1234abcd".into(),
            }
        );
    }
}

#[test]
fn parser_rejects_legacy_invalid_and_traversing_ids() {
    let sealed = "---\nkind: document\nof: doc/a.md\nfile: sha256:a\ncites: {}\n---\n\n# T\n";
    for id in [
        "product",
        "engineering",
        "product/section_01",
        "overview/Bad/product",
        "overview/a/other",
        "plans/product/section_01",
        "a/product/section_1",
        "overview/../product",
        "a/product/../../plans/p",
        "../plans/p",
    ] {
        let path = Path::new(STORE_DIR).join(format!("{id}.md"));
        assert!(parse(&path, sealed).is_err(), "{id}");
        let errors = parse_draft(id, "# T\n").unwrap_err();
        assert_eq!(errors[0].line, 1, "{id}");
        assert!(errors[0].message.contains("invalid summary id"), "{id}");
    }
}

#[test]
fn only_valid_ids_classify_as_sentence_targets() {
    let text = "# T\n\nA. [old](../product.md#s-1234abcd)\n\
                B. [old section](../product/section_01.md#s-1234abcd)\n\
                C. [bad feature](../overview/Bad/product.md#s-1234abcd)\n\
                D. [valid](../overview/alpha/product.md#s-1234abcd)\n";
    let body = parse_draft("plans/p", text).unwrap();
    let Block::Paragraph(sentences) = &body.blocks[0] else {
        panic!("expected a paragraph");
    };
    for sentence in &sentences[..3] {
        assert!(matches!(
            sentence.links[0].resolved,
            LinkTarget::Unresolved(_)
        ));
    }
    assert_eq!(
        sentences[3].links[0].resolved,
        LinkTarget::Sentence {
            summary: "overview/alpha/product".into(),
            id: "s-1234abcd".into(),
        }
    );
}
