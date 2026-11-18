//! Tests for `crate::planning::implements`.

use crate::planning::implements::{implemented_section_paths, parse, Implements};
use std::path::{Path, PathBuf};

#[test]
fn section_paths_follow_declaration_order_and_skip_bad_tokens() {
    let paths = implemented_section_paths(
        Path::new("doc/plans/beta/PLAN-y.md"),
        "# Y\n\nImplements: section_04, chapter_2, [s](../product/section_01.md), section_04.md\n",
    );
    assert_eq!(
        paths,
        [
            PathBuf::from("doc/product/beta/section_04.md"),
            PathBuf::from("doc/product/beta/section_01.md"),
        ]
    );
}

#[test]
fn section_paths_need_a_relative_feature_plan_path() {
    let text = "# Y\n\nImplements: section_04\n";
    for path in [
        "/repo/doc/plans/beta/PLAN-y.md",
        "doc/product/beta/section_04.md",
        "doc/plans/beta/index.md",
    ] {
        let paths = implemented_section_paths(Path::new(path), text);
        assert!(paths.is_empty(), "{path}: {paths:?}");
    }
}

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

#[test]
fn anchors_are_kept_in_order_and_deduplicated() {
    let parsed = parse(
        "Implements: section_04#b, [s](section_02.md#a), section_04#b, section_04#a, \
         section_02#a\n",
    );
    assert_eq!(parsed.value, Implements::Sections(vec![4, 2]));
    assert_eq!(
        parsed.anchors,
        [
            (4, "b".to_string()),
            (2, "a".to_string()),
            (4, "a".to_string())
        ]
    );
    assert!(parsed.bad_tokens.is_empty(), "{:?}", parsed.bad_tokens);
}

#[test]
fn malformed_anchors_are_bad_tokens() {
    let parsed = parse("Implements: section_01#Up, section_02#.., section_03#, section_05\n");
    assert_eq!(parsed.value, Implements::Sections(vec![5]));
    assert_eq!(
        parsed.bad_tokens,
        ["section_01#Up", "section_02#..", "section_03#"]
    );
    assert!(parsed.anchors.is_empty());
}

#[test]
fn anchored_tokens_still_give_section_paths() {
    let paths = implemented_section_paths(
        Path::new("doc/plans/beta/PLAN-y.md"),
        "# Y\n\nImplements: section_04#a, section_04\n",
    );
    assert_eq!(paths, [PathBuf::from("doc/product/beta/section_04.md")]);
}

#[test]
fn whole_lists_sections_named_without_an_anchor() {
    let parsed = parse("Implements: section_04#a, section_02, section_04, section_07#x\n");
    assert_eq!(parsed.whole, [2, 4]);
    let claims: Vec<(u32, Option<String>)> = parsed
        .claims()
        .into_iter()
        .map(|claim| (claim.number, claim.anchor))
        .collect();
    assert_eq!(claims, [(4, None), (2, None), (7, Some("x".to_string()))]);
}

#[test]
fn claims_are_empty_for_none_and_for_a_missing_line() {
    assert!(parse("Implements: none\n").claims().is_empty());
    assert!(parse("# X\n").claims().is_empty());
}
