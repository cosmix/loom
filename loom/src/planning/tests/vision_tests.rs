//! Tests for `crate::planning::vision`.

use crate::planning::vision::{parse_sections_table, VisionProblem, VisionRow};

#[test]
fn parses_real_table_shape_in_order() {
    let text = "## Specification sections\n\nEach becomes a file.\n\n\
| #   | Section                          | Covers |\n\
| --- | -------------------------------- | ------ |\n\
| 01  | Workflow model and project state | States |\n\
| 02  | `loom plan` command and server   | CLI    |\n\
| 03  | Document model and conventions   | Model  |\n";
    assert_eq!(
        parse_sections_table(text),
        Ok(vec![
            VisionRow {
                number: 1,
                name: "Workflow model and project state".into(),
                line: 6
            },
            VisionRow {
                number: 2,
                name: "`loom plan` command and server".into(),
                line: 7
            },
            VisionRow {
                number: 3,
                name: "Document model and conventions".into(),
                line: 8
            },
        ])
    );
}

#[test]
fn ignores_tables_outside_the_matching_span() {
    let text = "## Specification sections\nNo table here.\n## Other\n| # | Name |\n| - | - |\n| 1 | Wrong |\n";
    assert_eq!(
        parse_sections_table(text),
        Err(VisionProblem::NoTable { heading_line: 0 })
    );
}

#[test]
fn ignores_fenced_headings_and_tables() {
    let text = "```md\n## Specification sections\n| # | Name |\n| - | - |\n| 1 | Wrong |\n```\n## specification SECTIONS\n```\n| # | Name |\n| - | - |\n| 2 | Wrong |\n```\n| # | Name |\n| - | - |\n| 3 | Right |\n";
    assert_eq!(
        parse_sections_table(text),
        Ok(vec![VisionRow {
            number: 3,
            name: "Right".into(),
            line: 14,
        }])
    );
}

#[test]
fn no_unfenced_heading_is_reported() {
    assert_eq!(
        parse_sections_table("```\n## Specification sections\n```\n"),
        Err(VisionProblem::NoHeading)
    );
}

#[test]
fn strips_backticks_and_keeps_first_duplicate() {
    let text = "## Specification sections\n| # | Name |\n| :-: | ---: |\n| `02` | First |\n| 02 | Second |\n| 1 | Next |\n";
    assert_eq!(
        parse_sections_table(text),
        Ok(vec![
            VisionRow {
                number: 2,
                name: "First".into(),
                line: 3
            },
            VisionRow {
                number: 1,
                name: "Next".into(),
                line: 5
            },
        ])
    );
}

#[test]
fn reports_heading_without_table() {
    assert_eq!(
        parse_sections_table("# Vision\n## Specification sections\nText\n"),
        Err(VisionProblem::NoTable { heading_line: 1 })
    );
}

#[test]
fn reports_first_table_without_numbered_rows() {
    let text = "## Specification sections\n| # | Name |\n| - | - |\n| x | No |\n\n| # | Name |\n| - | - |\n| 1 | Later |\n";
    assert_eq!(
        parse_sections_table(text),
        Err(VisionProblem::NoNumberedRows { heading_line: 0 })
    );
}

#[test]
fn header_only_table_lists_no_sections() {
    let text = "## Specification sections\n\n| # | Name |\n| --- | --- |\n";
    assert_eq!(parse_sections_table(text), Ok(vec![]));
}

#[test]
fn table_ended_by_a_blank_line_after_its_delimiter_lists_no_sections() {
    let text = "## Specification sections\n| # | Name |\n| - | - |\n\nNone yet.\n\n| # | Name |\n| - | - |\n| 1 | Later |\n";
    assert_eq!(parse_sections_table(text), Ok(vec![]));
}
