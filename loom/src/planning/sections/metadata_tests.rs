//! Tests for plan metadata spans.

use super::metadata::metadata_spans;
use super::{normalized_hash, Span};

fn parse(text: &str) -> (Option<Span>, Vec<(String, Span)>) {
    metadata_spans(text, &text.lines().collect::<Vec<_>>())
}

#[test]
fn absent_metadata_has_no_anchors() {
    assert_eq!(parse("# Plan\nNo metadata."), (None, vec![]));
}

#[test]
fn stages_have_line_ranges_and_hashes() {
    let text = "# Plan\n<!-- loom METADATA -->\n```yaml\nstages:\n  - id: alpha\n    name: A\n  - id: 'beta'\n    name: B\n```\n<!-- END loom METADATA -->\n";
    let (metadata, stages) = parse(text);
    assert_eq!(metadata, Some(Span { start_line: 1, end_line: 10, hash: normalized_hash("<!-- loom METADATA -->\n```yaml\nstages:\n  - id: alpha\n    name: A\n  - id: 'beta'\n    name: B\n```\n<!-- END loom METADATA -->\n") }));
    assert_eq!(
        stages,
        vec![
            (
                "alpha".into(),
                Span {
                    start_line: 4,
                    end_line: 6,
                    hash: normalized_hash("  - id: alpha\n    name: A\n")
                }
            ),
            (
                "beta".into(),
                Span {
                    start_line: 6,
                    end_line: 8,
                    hash: normalized_hash("  - id: 'beta'\n    name: B\n")
                }
            ),
        ]
    );
}

#[test]
fn crlf_and_nested_ids_preserve_stage_boundaries() {
    let text = "<!-- loom METADATA -->\r\n```yaml\r\nstages:\r\n  - id: one\r\n    children:\r\n      - id: nested\r\n  - id: \"two\"\r\n```\r\n<!-- END loom METADATA -->\r\n";
    let (metadata, stages) = parse(text);
    assert_eq!((metadata.unwrap().start_line, stages.len()), (0, 2));
    assert_eq!(
        stages[0],
        (
            "one".into(),
            Span {
                start_line: 3,
                end_line: 6,
                hash: normalized_hash("  - id: one\n    children:\n      - id: nested\n")
            }
        )
    );
    assert_eq!(
        stages[1],
        (
            "two".into(),
            Span {
                start_line: 6,
                end_line: 7,
                hash: normalized_hash("  - id: \"two\"\n")
            }
        )
    );
}
