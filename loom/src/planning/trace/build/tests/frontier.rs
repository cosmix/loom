//! Gap order, the frontier's order, its one gap per document and its cap of five.

use super::{node, section, vision, Fixture, LEDGER, METAPLAN, VISION};
use crate::planning::trace::{Gap, GapKind, NodeRef, Trace};

fn kinds_and_nodes(gaps: &[Gap]) -> Vec<(GapKind, NodeRef)> {
    gaps.iter()
        .map(|gap| (gap.kind, gap.node.clone()))
        .collect()
}

fn frontier(trace: &Trace) -> Vec<(GapKind, NodeRef)> {
    kinds_and_nodes(&trace.frontier)
}

#[test]
fn the_frontier_takes_gaps_by_kind_one_per_document() {
    let fx = Fixture::new(&[
        (VISION, &vision(&[1, 2])),
        (&section(1), "# S1\n\n## A\n\nText.\n\n## B\n\nText.\n"),
        (&section(2), "# S2\n\n## C\n\nText.\n"),
        (
            METAPLAN,
            "# E\n\n## 1. P (`PLAN-p.md`)\n\nCovers: section_02\n",
        ),
        (
            LEDGER,
            "# Findings\n\n## F-1 C\n\nAbout: section_02#c\nStatus: open\n\nText.\n",
        ),
    ]);
    fx.settle(&section(1));
    let trace = fx.trace();
    let entry = node(METAPLAN, Some(trace.entries[0].anchor.as_str()));
    let (one, two) = (section(1), section(2));
    let gaps = [
        (GapKind::Revisit, entry.clone()),
        (GapKind::Settle, node(&two, None)),
        (GapKind::Uncovered, node(&one, Some("a"))),
        (GapKind::Uncovered, node(&one, Some("b"))),
        (GapKind::Triage, node(LEDGER, None)),
    ];
    assert_eq!(kinds_and_nodes(&trace.gaps), gaps);
    let expected = [
        gaps[0].clone(),
        gaps[1].clone(),
        gaps[2].clone(),
        gaps[4].clone(),
    ];
    assert_eq!(frontier(&trace), expected);

    // A revised section shows its change, and its uncovered headings leave the frontier.
    fx.put(&one, &fx.read(&one).replace("## B", "## B\n\nNew."));
    let trace = fx.trace();
    let expected = [
        (GapKind::Revised, node(&one, None)),
        (GapKind::Revisit, entry),
        (GapKind::Settle, node(&two, None)),
        (GapKind::Triage, node(LEDGER, None)),
    ];
    assert_eq!(frontier(&trace), expected);
}

#[test]
fn the_frontier_holds_at_most_five_gaps_in_spine_order() {
    let numbers: Vec<u32> = (1..=7).collect();
    let texts: Vec<(String, String)> = numbers
        .iter()
        .map(|n| (section(*n), format!("# S{n}\n\n## H{n}\n\nText.\n")))
        .collect();
    let vision = vision(&numbers);
    let mut files: Vec<(&str, &str)> = vec![(VISION, &vision)];
    files.extend(
        texts
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    let trace = Fixture::new(&files).trace();
    let settle = |n: u32| (GapKind::Settle, node(&section(n), None));
    assert_eq!(frontier(&trace), (1..=5).map(settle).collect::<Vec<_>>());
    assert_eq!(trace.gaps.len(), 14);
    assert!(trace.gaps[..7]
        .iter()
        .all(|gap| gap.kind == GapKind::Settle));
}
