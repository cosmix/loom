//! A done feature: its vision, metaplan and plans are final and ask for no settling; its
//! product sections stay settleable, since a follow-up plan can still change them.

use super::{doc, section, vision, Fixture, METAPLAN, VISION};
use crate::planning::trace::{GapKind, SettleState};

const DONE_PLAN: &str = "doc/plans/demo/DONE-PLAN-p.md";

fn done_feature() -> Fixture {
    Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), "# S1\n\n## A\n\nText.\n"),
        (
            METAPLAN,
            "# E\n\n## 1. P (`PLAN-p.md`)\n\nCovers: section_01\n",
        ),
        (DONE_PLAN, "# P\n\nImplements: section_01\n\nBody.\n"),
    ])
}

#[test]
fn a_done_features_vision_metaplan_and_plans_are_not_settleable() {
    let fx = done_feature();
    let trace = fx.trace();
    for rel in [VISION, METAPLAN, DONE_PLAN] {
        assert!(!doc(&trace, rel).settleable, "{rel} should be final");
    }
    assert!(doc(&trace, &section(1)).settleable);
    // The draft section is still asked for; nothing else of the feature is.
    let settle: Vec<_> = trace
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::Settle)
        .map(|gap| gap.node.path.clone())
        .collect();
    assert_eq!(settle, [std::path::PathBuf::from(section(1))]);
}

#[test]
fn a_done_features_moved_vision_is_no_gap_while_a_moved_section_is() {
    let fx = done_feature();
    fx.settle(VISION);
    fx.settle(&section(1));
    fx.put(VISION, &format!("{}\nMore.\n", fx.read(VISION)));
    let one = section(1);
    fx.put(&one, &fx.read(&one).replace("Text.", "Other text."));
    let trace = fx.trace();
    // The settle state itself stays the hash comparison's truth.
    assert_eq!(doc(&trace, VISION).settle, SettleState::Revised);
    let revised: Vec<_> = trace
        .gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::Revised)
        .map(|gap| gap.node.path.clone())
        .collect();
    assert_eq!(revised, [std::path::PathBuf::from(one)]);
}

#[test]
fn a_metaplan_whose_plans_are_all_done_is_final_while_an_earlier_step_has_a_gap() {
    // No vision: the checklist's current step is the vision, but every plan is done.
    let fx = Fixture::new(&[
        (
            METAPLAN,
            "# E\n\n## 1. P (`PLAN-p.md`)\n\nCovers: section_01\n",
        ),
        (DONE_PLAN, "# P\n\nBody.\n"),
    ]);
    assert!(!doc(&fx.trace(), METAPLAN).settleable);
}

#[test]
fn an_unfinished_features_vision_and_metaplan_stay_settleable() {
    let fx = done_feature();
    fx.rename(DONE_PLAN, "doc/plans/demo/PLAN-p.md");
    let trace = fx.trace();
    assert!(doc(&trace, VISION).settleable);
    assert!(doc(&trace, METAPLAN).settleable);
    assert!(doc(&trace, "doc/plans/demo/PLAN-p.md").settleable);
}
