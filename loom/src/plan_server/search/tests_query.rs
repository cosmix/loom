//! Tests for search::query.

use super::query::{literal_count, literal_snippet, parse_query, snippet, Query};

#[test]
fn parses_quoted_literals_and_tokenizes_other_queries() {
    assert_eq!(
        parse_query("  \"IN_PROGRESS-\"  "),
        Query::Literal("IN_PROGRESS-".into())
    );
    assert_eq!(parse_query("--strict"), Query::Terms(vec!["strict".into()]));
    assert_eq!(parse_query("  "), Query::Terms(vec![]));
    assert_eq!(
        parse_query("\"unfinished"),
        Query::Terms(vec!["unfinished".into()])
    );
}

#[test]
fn counts_case_sensitive_non_overlapping_literals() {
    assert_eq!(
        literal_count("IN_PROGRESS- in_progress- IN_PROGRESS-", "IN_PROGRESS-"),
        2
    );
    assert_eq!(literal_count("aaaa", "aa"), 2);
    assert_eq!(literal_count("text", ""), 0);
}

#[test]
fn snippet_centers_first_match_on_character_boundaries() {
    let body = "αβγδεζηθ TARGET λμνξοπρσ";
    assert_eq!(snippet(body, &["target".into()], 10), "…εζηθ TARGE…");
    assert_eq!(snippet("αβγδε", &[], 3), "αβγ…");
    assert_eq!(snippet("αβγ", &["β".into()], 10), "αβγ");
    assert_eq!(snippet(body, &[], 0), "");
}

#[test]
fn a_literal_snippet_centers_on_the_case_sensitive_match() {
    let body = format!("quartz {} Quartz tail", "x".repeat(300));
    let shown = literal_snippet(&body, "Quartz", 20);
    assert!(
        shown.contains("Quartz") && !shown.contains("quartz"),
        "{shown}"
    );
    // Terms are matched without regard to case, so the same window starts at the first one.
    assert!(snippet(&body, &["Quartz".into()], 20).contains("quartz"));
    assert_eq!(literal_snippet("αβγδε", "", 3), "αβγ…");
    assert_eq!(literal_snippet("αβγ", "β", 0), "");
}
