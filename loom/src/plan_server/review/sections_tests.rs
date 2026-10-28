//! Tests for changed sections of a turn.

use super::sections::*;

#[test]
fn changed_heading_listed() {
    let start = "# Overview\nBefore\n";
    let end = "# Overview\nAfter\n";

    let changed = turn_sections(start, end);

    assert_eq!(changed, ["overview"]);
}

#[test]
fn unchanged_heading_omitted() {
    let start = "# Changed\nBefore\n# Stable\nSame\n";
    let end = "# Changed\nAfter\nExtra line\n# Stable\nSame\n";

    let changed = turn_sections(start, end);

    assert_eq!(changed, ["changed"]);
}

#[test]
fn added_and_removed_headings_listed() {
    let start = "# Removed Z\nOld\n# Retained\nBefore\n# Removed A\nOld\n";
    let end = "# Added Z\nNew\n# Retained\nAfter\n# Added A\nNew\n";

    let changed = turn_sections(start, end);

    assert_eq!(
        changed,
        ["added-z", "retained", "added-a", "removed-z", "removed-a"]
    );
}

#[test]
fn preamble_change_listed_as_empty_anchor() {
    let start = "Before\n# Stable\nSame\n";
    let end = "After\n# Stable\nSame\n";

    let changed = turn_sections(start, end);

    assert_eq!(changed, [""]);
}

#[test]
fn moved_headings_omitted() {
    let start = "# First\nOne\n# Second\nTwo\n";
    let end = "# Second\nTwo\n# First\nOne\n";

    let changed = turn_sections(start, end);

    assert!(changed.is_empty());
}

#[test]
fn normalized_whitespace_changes_omitted() {
    let start = "Preamble\n# Heading\nBody\n";
    let end = "Preamble \t\r\n# Heading \t\r\nBody \t\r\n";

    let changed = turn_sections(start, end);

    assert!(changed.is_empty());
}
