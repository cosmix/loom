//! Tests for `crate::planning::run_review`.

use super::*;
use crate::fs::plan_review::review_item_line;

fn item(name: &str, id: &str, text: &str) -> ReviewItem {
    ReviewItem {
        stage_name: name.into(),
        stage_id: id.into(),
        text: text.into(),
    }
}

fn review(findings: &[String]) -> String {
    format!(
        "# Code Review: X\n\n## Open Questions\n\n- **Q** (`q`): a question\n\n\
         ## Open Findings\n\n{}\n\n## Later\n\n- **L** (`l`): after the section\n",
        findings.join("\n")
    )
}

#[test]
fn a_name_holding_markup_and_a_text_holding_delimiters_round_trip() {
    let name = "Fix **it** \\ now: (x)";
    let text = "keep `): ` and ** as written";
    let doc = review(&[review_item_line(name, "fix_it-2", text)]);
    assert_eq!(
        review_items(&doc, ReviewSection::OpenFindings),
        [item(name, "fix_it-2", text)]
    );
}

#[test]
fn malformed_bullets_are_skipped() {
    let lines = [
        "- **Good** (`good`): kept".to_string(),
        "- **Unclosed name (`x`): dropped".to_string(),
        "- **Bad id** (`a b`): dropped".to_string(),
        "- **Empty id** (``): dropped".to_string(),
        "- **No colon** (`x`) dropped".to_string(),
        "* **Wrong bullet** (`x`): dropped".to_string(),
        "No open findings.".to_string(),
    ];
    let items = review_items(&review(&lines), ReviewSection::OpenFindings);
    assert_eq!(items, [item("Good", "good", "kept")]);
}

#[test]
fn sections_stop_at_the_next_heading_and_an_absent_section_is_empty() {
    let doc = review(&[]);
    assert_eq!(
        review_items(&doc, ReviewSection::OpenQuestions),
        [item("Q", "q", "a question")]
    );
    assert!(review_items(&doc, ReviewSection::OpenFindings).is_empty());
}

#[test]
fn a_heading_line_inside_an_earlier_section_does_not_start_the_section() {
    let doc = format!(
        "# Code Review: X\n\n## Changes by Stage\n\n- **S** (`s`): did a thing\n## Open Findings\n\
         - **Fake** (`fake`): inside the changes\n\n{}",
        "## Open Findings\n\n- **Real** (`real`): the finding\n"
    );
    assert_eq!(
        review_items(&doc, ReviewSection::OpenFindings),
        [item("Real", "real", "the finding")]
    );
}
