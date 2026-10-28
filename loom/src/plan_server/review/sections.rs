//! Changed sections of a turn.

use crate::planning::sections::sections_of;
use std::collections::{HashMap, HashSet};

/// Changed heading anchors, followed by removed anchors; `""` addresses the preamble.
pub(crate) fn turn_sections(start: &str, end: &str) -> Vec<String> {
    let start = sections_of(start);
    let end = sections_of(end);
    let mut changed = Vec::new();
    if start.preamble.hash != end.preamble.hash {
        changed.push(String::new());
    }

    let start_hashes: HashMap<_, _> = start
        .headings
        .iter()
        .map(|heading| (heading.anchor.as_str(), &heading.span.hash))
        .collect();
    let mut end_anchors = HashSet::new();
    for heading in &end.headings {
        end_anchors.insert(heading.anchor.as_str());
        if start_hashes.get(heading.anchor.as_str()).copied() != Some(&heading.span.hash) {
            changed.push(heading.anchor.clone());
        }
    }
    for heading in &start.headings {
        if !end_anchors.contains(heading.anchor.as_str()) {
            changed.push(heading.anchor.clone());
        }
    }
    changed
}
