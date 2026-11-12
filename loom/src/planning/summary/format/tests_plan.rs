//! Tests for link classification and plan caps in the summary format.

use super::*;

#[test]
fn a_link_into_a_metaplan_summary_is_a_sentence_and_one_into_its_source_a_passage() {
    assert_eq!(
        classify_href("plans/x", "../alpha/metaplan/index.md#s-0123abcd"),
        LinkTarget::Sentence {
            summary: "alpha/metaplan/index".into(),
            id: "s-0123abcd".into(),
        }
    );
    assert_eq!(
        classify_href(
            "alpha/metaplan/index",
            "../../../../plans/alpha/index.md#sequencing"
        ),
        LinkTarget::Source {
            path: "doc/plans/alpha/index.md".into(),
            anchor: "sequencing".into(),
        }
    );
    assert!(matches!(
        classify_href("plans/x", "../alpha/metaplan/Index.md#s-0123abcd"),
        LinkTarget::Unresolved(_)
    ));
}

#[test]
fn plan_caps_grow_with_the_source_between_their_floor_and_ceiling() {
    let caps = |lines| PlanCaps::for_source_lines(lines);
    assert_eq!(
        caps(0),
        PlanCaps {
            opening: 8,
            detail: 15
        }
    );
    assert_eq!(
        caps(24 * 15),
        PlanCaps {
            opening: 8,
            detail: 15
        }
    );
    assert_eq!(
        caps(24 * 40),
        PlanCaps {
            opening: 8,
            detail: 40
        }
    );
    assert_eq!(
        caps(24 * 60),
        PlanCaps {
            opening: 12,
            detail: 60
        }
    );
    assert_eq!(
        caps(1_000_000),
        PlanCaps {
            opening: 12,
            detail: 60
        }
    );
}
