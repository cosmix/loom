//! Tests for `crate::planning::implements`.

use crate::planning::implements::{parse, Implements};

#[test]
fn token_forms_and_link_sides() {
    let parsed = parse("# P\n\nImplements: section_07, [section_08](../product/section_08.md), section_09.md, [section_10](elsewhere.md), [label](../product/section_11.md)\n");
    assert_eq!(parsed.value, Implements::Sections(vec![7, 8, 9, 10, 11]));
    assert_eq!(parsed.line, Some(2));
    assert!(parsed.bad_tokens.is_empty());
    assert!(parsed.duplicate_lines.is_empty());
}

#[test]
fn none_is_case_insensitive_and_unmapped_is_distinct() {
    for value in ["none", "NONE"] {
        assert_eq!(
            parse(&format!("Implements: {value}\n")).value,
            Implements::NoneDeclared
        );
    }
    assert_eq!(parse("# P\nNo line here.\n").value, Implements::Unmapped);
}

#[test]
fn ignores_fences_and_metadata() {
    let text = "```text\nImplements: section_01\n<!-- loom METADATA -->\n```\nImplements: section_02\n<!-- loom METADATA -->\nImplements: section_03\n";
    let parsed = parse(text);
    assert_eq!(parsed.value, Implements::Sections(vec![2]));
    assert_eq!(parsed.line, Some(4));
    assert!(parsed.duplicate_lines.is_empty());
    assert_eq!(
        parse("```\nImplements: section_01\n```\n").value,
        Implements::Unmapped
    );
}

#[test]
fn first_declaration_wins() {
    let parsed = parse("# P\n\nImplements: section_01\nImplements: bad\n");
    assert_eq!(parsed.value, Implements::Sections(vec![1]));
    assert_eq!((parsed.line, parsed.duplicate_lines), (Some(2), vec![3]));
    assert!(parsed.bad_tokens.is_empty());
}

#[test]
fn reports_invalid_and_empty_tokens() {
    let parsed = parse("Implements: section_7x, chapter_2, section_03, none\n");
    assert_eq!(parsed.value, Implements::Sections(vec![3]));
    assert_eq!(parsed.bad_tokens, ["section_7x", "chapter_2", "none"]);
    let empty = parse("Implements:   \n");
    assert_eq!(empty.value, Implements::Sections(vec![]));
    assert_eq!(empty.bad_tokens, [""]);
}
