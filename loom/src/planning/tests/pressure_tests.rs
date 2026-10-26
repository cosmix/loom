//! Tests for `crate::planning::pressure`.

use crate::planning::pressure::{open_questions, records, PressureRecord};

#[test]
fn records_only_valid_dates_in_pressure_sections() {
    let text = "- 2026-01-01: outside\n## Pressure tests\n\
- 2026-10-26: first review\n- 2026-13-01: bad month\n\
- 2026-02-00: bad day\n- 2026-1-02: malformed\n\
- 2026-11-02: second review\n## Other\n- 2026-01-01: outside\n";
    assert_eq!(
        records(text),
        vec![
            PressureRecord {
                date: "2026-10-26".into(),
                line: 2,
                text: "first review".into()
            },
            PressureRecord {
                date: "2026-11-02".into(),
                line: 6,
                text: "second review".into()
            },
        ]
    );
}

#[test]
fn questions_sum_sections_and_ignore_nested_items() {
    let text = "## Open questions\n- first\n  - nested\n* second\n\
## Other\n- outside\n### OPEN QUESTIONS\n1. third\n12. fourth\n   2. nested\n";
    assert_eq!(open_questions(text), 4);
}

#[test]
fn fenced_examples_and_headings_are_ignored() {
    let text = "```markdown\n## Pressure tests\n- 2026-01-01: example\n\
## Open questions\n- example\n```\n## Pressure tests\n```\n\
- 2026-01-01: example\n```\n- 2026-01-31: real\n\
## Open questions\n```\n- example\n```\n- real\n";
    assert_eq!(records(text).len(), 1);
    assert_eq!(open_questions(text), 1);
}

#[test]
fn overlapping_question_sections_count_each_item_once() {
    let text = "## Open questions\n- first\n### Open questions\n- second\n";
    assert_eq!(open_questions(text), 2);
}
