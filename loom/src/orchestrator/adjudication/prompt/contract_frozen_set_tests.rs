//! Tests for the frozen-set section of the contract dispute briefing: the
//! harness files an accept would newly freeze.

use std::fs;

use super::super::tests::{
    contract_fixture, freeze_then_change_support_files, frozen_set_of, harness_fixture,
};

/// An accept freezes every harness match the record lacks, so the judge sees
/// each one, cut to the per-file bound.
#[test]
fn the_briefing_shows_each_new_harness_file_an_accept_would_freeze() {
    let fx = harness_fixture();
    let new = format!(
        "pub fn fresh() -> u8 {{ 7 }}\n{}",
        "// padding\n".repeat(1_000)
    );
    fs::write(fx.worktree.join("tests/support/new.rs"), new).unwrap();

    let section = frozen_set_of(&fx);

    for expected in [
        "- `tests/support/new.rs` is new and will be frozen:",
        "pub fn fresh() -> u8 { 7 }",
        "[content truncated]",
    ] {
        assert!(section.contains(expected), "missing {expected}:\n{section}");
    }
    assert!(section.len() < 12 * 1024, "{} bytes", section.len());
    assert!(!section.contains("old.rs"), "{section}");
    assert!(!section.contains("x_contract.rs"), "{section}");
}

#[test]
fn a_harness_listing_failure_is_a_note_not_a_failed_briefing() {
    let mut fx = contract_fixture();
    freeze_then_change_support_files(&fx);
    fx.stage.harness = vec!["tests/support/**".to_string()];

    let section = frozen_set_of(&fx);

    assert!(
        section.contains("(new harness files not listed:"),
        "{section}"
    );
    assert!(
        section.contains("- `tests/support/gone.rs` is deleted"),
        "{section}"
    );
}
