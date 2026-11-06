//! Tests for `crate::planning::trace::tokens`.

use super::{section_refs, stage_lines, tokens, SectionRef, Token};

fn section(number: u32, anchor: Option<&str>) -> SectionRef {
    SectionRef {
        number,
        anchor: anchor.map(str::to_owned),
    }
}

#[test]
fn section_tokens_in_every_form() {
    let (refs, bad) = section_refs(
        "section_04, section_05.md, section_06#rebuild-budget, [section_07](x.md), \
         [Alpha](../product/demo/section_08.md#two), [Beta](section_09.md)",
    );
    assert_eq!(
        refs,
        [
            section(4, None),
            section(5, None),
            section(6, Some("rebuild-budget")),
            section(7, None),
            section(8, Some("two")),
            section(9, None),
        ]
    );
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn refs_are_deduplicated_and_empty_tokens_are_bad() {
    let (refs, bad) = section_refs("section_01#a, section_01#a, section_01, section_01.md, ,x,");
    assert_eq!(refs, [section(1, Some("a")), section(1, None)]);
    assert_eq!(bad, ["", "x", ""]);
    let (refs, bad) = section_refs("a,,b");
    assert!(refs.is_empty());
    assert_eq!(bad, ["a", "", "b"]);
}

#[test]
fn anchors_must_be_lowercase_slugs_or_stage_ids() {
    for bad in [
        "section_01#..",
        "section_01#../x",
        "section_01#Upper",
        "section_01#has space",
        "section_01#",
        "section_01#a.b",
        "PLAN-p.md#stage:",
        "PLAN-p.md#stage:a/b",
    ] {
        let (found, rejected) = tokens(bad);
        assert!(found.is_empty(), "{bad}: {found:?}");
        assert_eq!(rejected, [bad]);
    }
    let (found, rejected) = tokens("PLAN-p.md#stage:Build_2, section_01#a-b_c1");
    assert!(rejected.is_empty(), "{rejected:?}");
    assert_eq!(found.len(), 2);
}

#[test]
fn general_tokens_name_vision_metaplan_and_plans() {
    let (found, bad) = tokens(
        "vision, vision#goals, metaplan#1-first-plan-planmd, DONE-PLAN-x-y.md, \
         [the plan](../PLAN-p.md#stage:build), IN_PROGRESS-PLAN-q.md#notes, metaplan",
    );
    assert!(bad.is_empty(), "{bad:?}");
    let plan = |slug: &str, anchor: Option<&str>| Token::Plan {
        slug: slug.into(),
        anchor: anchor.map(str::to_owned),
    };
    assert_eq!(
        found,
        [
            Token::Vision(None),
            Token::Vision(Some("goals".into())),
            Token::Metaplan(Some("1-first-plan-planmd".into())),
            plan("x-y", None),
            plan("p", Some("stage:build")),
            plan("q", Some("notes")),
            Token::Metaplan(None),
        ]
    );
}

#[test]
fn section_refs_reject_other_node_kinds() {
    let (refs, bad) = section_refs("section_01, vision, PLAN-a.md, nonsense");
    assert_eq!(refs, [section(1, None)]);
    assert_eq!(bad, ["vision", "PLAN-a.md", "nonsense"]);
}

#[test]
fn stage_lines_skip_fences_and_metadata() {
    let text = "# P\n\nImplements (stage:build): section_01#a, bogus\n```text\n\
                Implements (stage:fenced): section_02\n```\n  Implements (stage:indented): section_03\n\
                Implements (stage: lint ): section_02, section_02\nImplements (stage:open: x\n\
                <!-- loom METADATA -->\nImplements (stage:late): section_04\n";
    let found = stage_lines(text);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].stage, "build");
    assert_eq!(found[0].refs, [section(1, Some("a"))]);
    assert_eq!(found[0].bad, ["bogus"]);
    assert_eq!(found[0].line, 2);
    assert_eq!(found[1].stage, "lint");
    assert_eq!(found[1].refs, [section(2, None)]);
    assert_eq!(found[1].line, 7);
}
