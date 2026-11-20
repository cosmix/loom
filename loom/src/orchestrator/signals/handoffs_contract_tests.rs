//! Contract for the integration-verify signal's "Handed-Off Tests" section:
//! every test an adjudicator left out of a stage's check is listed with where
//! it runs, an unverified hand-off says so, and an empty list adds nothing.

use super::handoffs::{append_handoffs, AmendmentLine};
use crate::verify::review::handoffs::Handoff;

const HEADING: &str = "## Handed-Off Tests";
const PRELUDE: &str = "# Signal\n\nExisting content.\n\n";

fn handoff(stage: &str, tests: &[&str], runs_at: &str, reason: &str, verified: bool) -> Handoff {
    Handoff {
        stage: stage.to_string(),
        field: "acceptance".to_string(),
        index: 0,
        tests: tests.iter().map(|test| test.to_string()).collect(),
        runs_at: runs_at.to_string(),
        reason: reason.to_string(),
        dispute: format!("dispute-{stage}"),
        verified,
    }
}

/// The one line of `content` holding `needle`.
fn line_with<'a>(content: &'a str, needle: &str) -> &'a str {
    let lines: Vec<&str> = content.lines().filter(|l| l.contains(needle)).collect();
    assert_eq!(lines.len(), 1, "one line must hold {needle:?}:\n{content}");
    lines[0]
}

fn assert_line_holds(line: &str, parts: &[&str]) {
    for part in parts {
        assert!(line.contains(part), "line must hold {part:?}: {line}");
    }
}

fn verified_and_unverified() -> [Handoff; 2] {
    [
        handoff(
            "stage-alpha",
            &["alpha_needs_network", "beta_needs_network"],
            "host-with-network",
            "requires outbound network",
            true,
        ),
        handoff(
            "stage-beta",
            &["gamma_slow_suite"],
            "nightly-runner",
            "base run timed out",
            false,
        ),
    ]
}

fn amendment() -> AmendmentLine {
    AmendmentLine {
        stage: "stage-gamma".to_string(),
        field: "acceptance".to_string(),
        op: "replace".to_string(),
        index: 2,
        reason: "flag was renamed upstream".to_string(),
    }
}

#[test]
fn handoffs_are_listed_for_integration_verify() {
    let handoffs = verified_and_unverified();
    let amendments = [amendment()];
    let mut content = PRELUDE.to_string();
    append_handoffs(&mut content, &handoffs, &amendments);

    assert!(
        content.starts_with(PRELUDE),
        "prior content kept:\n{content}"
    );
    assert_eq!(
        content.matches(HEADING).count(),
        1,
        "one heading:\n{content}"
    );

    let verified = line_with(&content, "alpha_needs_network");
    let verified_parts = [
        "stage-alpha",
        "beta_needs_network",
        "host-with-network",
        "requires outbound network",
    ];
    assert_line_holds(verified, &verified_parts);
    assert!(!verified.contains("unverified"), "verified: {verified}");

    let unverified = line_with(&content, "gamma_slow_suite");
    let unverified_parts = [
        "stage-beta",
        "nightly-runner",
        "base run timed out",
        "unverified",
    ];
    assert_line_holds(unverified, &unverified_parts);

    let amendment = line_with(&content, "flag was renamed upstream");
    assert_line_holds(amendment, &["stage-gamma", "replace"]);

    let mut empty = PRELUDE.to_string();
    append_handoffs(&mut empty, &[], &[]);
    assert_eq!(empty, PRELUDE, "nothing to hand off adds nothing");
}
