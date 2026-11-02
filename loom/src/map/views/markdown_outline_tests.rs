//! Tests for the direct Markdown outline.

use std::fs;

use tempfile::TempDir;

use super::markdown_outline::{is_markdown_path, render, to_json};

fn outline_of(body: &str) -> String {
    let dir = TempDir::new().expect("temp dir");
    let file = dir.path().join("doc.md");
    fs::write(&file, body).expect("write doc.md");
    render(dir.path(), file.to_str().expect("utf-8 path"))
}

fn rows(outline: &str) -> Vec<&str> {
    outline.lines().filter(|l| l.starts_with("  L")).collect()
}

#[test]
fn recognizes_markdown_extensions_ignoring_case() {
    for path in ["a.md", "dir/B.MD", "c.markdown", "d.Markdown"] {
        assert!(is_markdown_path(path), "{path}");
    }
    for path in ["a.rs", "md", "a.md.txt", "notes"] {
        assert!(!is_markdown_path(path), "{path}");
    }
}

#[test]
fn nested_levels_carry_scope_and_section_ranges() {
    let outline = outline_of("# Top\n\n## Mid\n\n### Deep\n\ntext\n\n## Next\n\ntext\n");
    let rows = rows(&outline);
    assert_eq!(rows.len(), 4, "{outline}");
    assert!(rows[0].starts_with("  L1-L11 "), "{}", rows[0]);
    assert!(rows[1].starts_with("  L3-L8 "), "{}", rows[1]);
    assert!(rows[1].contains("heading"), "{}", rows[1]);
    assert!(rows[1].contains("Top"), "{}", rows[1]);
    assert!(rows[2].starts_with("  L5-L8 "), "{}", rows[2]);
    assert!(rows[2].contains("Top::Mid"), "{}", rows[2]);
    assert!(rows[2].ends_with("### Deep"), "{}", rows[2]);
    assert!(rows[3].starts_with("  L9-L11 "), "{}", rows[3]);
    assert!(
        outline.ends_with("coverage: markdown headings"),
        "{outline}"
    );
}

#[test]
fn a_longer_fence_is_closed_only_by_an_equal_or_longer_one() {
    let body = "# Real\n\n````\n```\n# inner\n```\n## still code\n````\n\n## After\n";
    let outline = outline_of(body);
    let rows = rows(&outline);
    assert_eq!(rows.len(), 2, "{outline}");
    assert!(rows[0].contains("Real"), "{}", rows[0]);
    assert!(rows[1].contains("After"), "{}", rows[1]);
    assert!(!outline.contains("inner") && !outline.contains("still code"));
}

#[test]
fn a_closing_hash_run_is_not_part_of_the_title() {
    let outline = outline_of("# Top ##\n\n## C#\n\n### Sub\n");
    let rows = rows(&outline);
    assert_eq!(rows.len(), 3, "{outline}");
    assert!(rows[2].contains("Top::C#"), "{}", rows[2]);
    // The signature keeps the heading's source text, closing run included.
    assert!(rows[0].ends_with("# Top ##"), "{}", rows[0]);
}

#[test]
fn an_empty_heading_closes_the_scope_it_interrupts() {
    let outline = outline_of("# A\n#\n## C\n");
    let rows = rows(&outline);
    assert_eq!(rows.len(), 2, "{outline}");
    assert!(rows[0].starts_with("  L1-L1 "), "{}", rows[0]);
    assert!(rows[1].starts_with("  L3-L3 "), "{}", rows[1]);
    assert!(!rows[1].contains("A::"), "{}", rows[1]);
}

#[test]
fn an_empty_file_has_a_header_and_coverage_but_no_rows() {
    let outline = outline_of("");
    assert!(outline.contains("Outline:"), "{outline}");
    assert!(rows(&outline).is_empty(), "{outline}");
    assert!(
        outline.ends_with("coverage: markdown headings"),
        "{outline}"
    );
}

#[test]
fn rows_are_capped_at_two_hundred() {
    let body: String = (1..=201).map(|n| format!("# H{n}\n")).collect();
    let outline = outline_of(&body);
    assert_eq!(rows(&outline).len(), 200, "{outline}");
    assert!(outline.contains("\n... 1 more headings\n"), "{outline}");
    assert!(!outline.contains("# H201"), "{outline}");
    assert!(outline.contains("# H200"), "{outline}");
    assert!(
        outline.ends_with("coverage: markdown headings"),
        "{outline}"
    );
}

#[test]
fn a_missing_file_is_unavailable_without_rows() {
    let dir = TempDir::new().expect("temp dir");
    let outline = render(dir.path(), "absent.md");
    assert!(rows(&outline).is_empty(), "{outline}");
    assert!(
        outline.contains("coverage: markdown unavailable - "),
        "{outline}"
    );
}

#[test]
fn json_mirrors_the_graph_outline_shape() {
    let dir = TempDir::new().expect("temp dir");
    let file = dir.path().join("doc.md");
    fs::write(&file, "# Top\n\n## Mid\n").expect("write doc.md");
    let value = to_json(dir.path(), file.to_str().expect("utf-8 path"));
    assert_eq!(value["path"], "doc.md");
    assert_eq!(value["coverage"]["status"], "markdown headings");
    let symbols = value["symbols"].as_array().expect("symbols");
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[1]["id"], "doc.md#L3");
    assert_eq!(symbols[1]["kind"], "heading");
    assert_eq!(symbols[1]["scope"], serde_json::json!(["Top"]));
    assert_eq!(symbols[1]["line_start"], 3);
    assert_eq!(symbols[1]["line_end"], 3);
    assert_eq!(symbols[1]["signature"], "## Mid");
}
