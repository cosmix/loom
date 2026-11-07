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

/// The planner reads what `loom pressure` writes: `record::append_to_text` (behind
/// `append_record`, the one writer both drivers use) placed after a plan's metadata, a first
/// record creating the section and a second joining it, with LF and CRLF line ends.
#[test]
fn records_read_what_the_pressure_pipeline_writes() {
    use crate::pressure::record::{append_to_text, RecordLine};
    let line = |day: u32| RecordLine {
        date: chrono::NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
        claude_model: "opus".into(),
        codex_models: vec!["gpt-6-astra".into()],
        rounds: 1,
        missing_rounds: Vec::new(),
    };
    let plan = "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  stages:\n    - id: a\n      \
description: |\n        ## Pressure tests\n        - 2026-01-01: inside YAML\n```\n\n\
<!-- END loom METADATA -->\n";
    for text in [plan.to_owned(), plan.replace('\n', "\r\n")] {
        let once = append_to_text(&text, &line(3).render());
        let twice = append_to_text(&once, &line(7).render());
        let dates: Vec<_> = records(&twice).into_iter().map(|r| r.date).collect();
        assert_eq!(dates, ["2026-11-03", "2026-11-07"], "{twice:?}");
    }
}
