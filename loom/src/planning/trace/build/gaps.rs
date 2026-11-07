//! Gaps and the frontier.

use std::path::Path;

use super::{count, metaplan_path, Cx};
use crate::planning::findings::{ledger_path, FindingStatus};
use crate::planning::trace::{
    DocRole, Gap, GapKind, NodeRef, SettleState, TraceDoc, TraceEntry, TraceFinding, TraceHeading,
};

/// How many gaps the frontier shows.
const FRONTIER: usize = 5;

/// The settle states that are gaps of a settleable document, in frontier order.
const CHANGED: [(SettleState, GapKind); 3] = [
    (SettleState::Revised, GapKind::Revised),
    (SettleState::StubChanged, GapKind::StubChanged),
    (SettleState::UpstreamChanged, GapKind::UpstreamChanged),
];

/// Every gap, by kind in `GapKind` order and then in spine order.
pub(super) fn gaps(
    cx: &Cx,
    documents: &[TraceDoc],
    headings: &[TraceHeading],
    entries: &[TraceEntry],
    findings: &[TraceFinding],
) -> Vec<Gap> {
    // Only a document the reader is asked to settle has its settle state as a gap.
    let settleable: Vec<&TraceDoc> = documents.iter().filter(|doc| doc.settleable).collect();
    let mut found = Vec::new();
    for (state, kind) in CHANGED {
        let changed = settleable.iter().filter(|doc| doc.settle == state);
        found.extend(changed.map(|doc| gap(kind, whole(&doc.path), None)));
    }
    found.extend(revisit_gaps(cx, entries));
    let drafts = settleable.iter().filter(|doc| {
        matches!(doc.role, DocRole::Section | DocRole::Plan) && doc.settle == SettleState::Draft
    });
    found.extend(drafts.map(|doc| gap(GapKind::Settle, whole(&doc.path), None)));
    let uncovered = headings
        .iter()
        .filter(|heading| heading.covered_by.is_empty());
    found.extend(uncovered.map(|heading| {
        let node = NodeRef {
            path: heading.path.clone(),
            anchor: Some(heading.anchor.clone()),
        };
        gap(GapKind::Uncovered, node, None)
    }));
    let open = findings
        .iter()
        .filter(|finding| finding.finding.status == FindingStatus::Open)
        .count();
    if open > 0 {
        let node = whole(&ledger_path(cx.name()));
        found.push(gap(GapKind::Triage, node, Some(count(open))));
    }
    found
}

/// An entry with findings to revisit: its written plan (revisit before running), else the
/// entry itself (revisit before writing).
fn revisit_gaps(cx: &Cx, entries: &[TraceEntry]) -> Vec<Gap> {
    let metaplan = metaplan_path(cx.name());
    let pending = entries.iter().filter(|entry| !entry.revisit.is_empty());
    pending
        .map(|entry| {
            let node = match &entry.plan {
                Some(plan) => whole(plan),
                None => NodeRef {
                    path: metaplan.clone(),
                    anchor: Some(entry.anchor.clone()),
                },
            };
            gap(GapKind::Revisit, node, Some(count(entry.revisit.len())))
        })
        .collect()
}

/// The first five gaps, one per document, uncovered headings only of settled sections.
pub(super) fn frontier(gaps: &[Gap], documents: &[TraceDoc]) -> Vec<Gap> {
    let settled = |path: &Path| {
        let doc = documents.iter().find(|doc| doc.path == path);
        doc.is_some_and(|doc| doc.settle == SettleState::Settled)
    };
    let mut seen: Vec<&Path> = Vec::new();
    let mut out = Vec::new();
    for gap in gaps {
        if out.len() == FRONTIER {
            break;
        }
        let path = gap.node.path.as_path();
        if seen.contains(&path) || (gap.kind == GapKind::Uncovered && !settled(path)) {
            continue;
        }
        seen.push(path);
        out.push(gap.clone());
    }
    out
}

fn gap(kind: GapKind, node: NodeRef, count: Option<u32>) -> Gap {
    Gap { kind, node, count }
}

fn whole(path: &Path) -> NodeRef {
    NodeRef {
        path: path.to_path_buf(),
        anchor: None,
    }
}
