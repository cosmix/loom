use std::path::Path;

use serde_json::{json, Value};

use super::{patch_counts, result_text, summary, unified_patch};

fn summarize(tool: &str, input: Value) -> String {
    summary(tool, &input, Path::new("/project"))
}

fn structured_patch() -> Value {
    json!([
        {"oldStart": 40, "oldLines": 7, "newStart": 40, "newLines": 10,
         "lines": [" ctx", "-old", "+new"]},
        {"oldStart": 60, "oldLines": 0, "newStart": 63, "newLines": 2,
         "lines": ["+first", "+second"]}
    ])
}

#[test]
fn read_shows_relative_path_and_inclusive_range() {
    let input = json!({"file_path": "/project/src/lib.rs", "offset": 40, "limit": 7});

    assert_eq!(summarize("Read", input), "src/lib.rs:40-46");
}

#[test]
fn read_without_range_shows_path() {
    assert_eq!(
        summarize("Read", json!({"file_path": "src/lib.rs"})),
        "src/lib.rs"
    );
}

#[test]
fn read_with_only_limit_starts_at_first_line() {
    let input = json!({"file_path": "src/lib.rs", "limit": 7});

    assert_eq!(summarize("Read", input), "src/lib.rs:1-7");
}

#[test]
fn read_with_only_offset_shows_start_line() {
    let input = json!({"file_path": "src/lib.rs", "offset": 40});

    assert_eq!(summarize("Read", input), "src/lib.rs:40-40");
}

#[test]
fn read_range_does_not_overflow() {
    let input = json!({"file_path": "f", "offset": u64::MAX, "limit": 2});

    assert_eq!(
        summarize("Read", input),
        "f:18446744073709551615-18446744073709551615"
    );
}

#[test]
fn edit_shows_relative_path() {
    assert_eq!(
        summarize("Edit", json!({"file_path": "/project/src/a.rs"})),
        "src/a.rs"
    );
}

#[test]
fn write_shows_relative_path() {
    assert_eq!(
        summarize("Write", json!({"file_path": "/project/doc/a.md"})),
        "doc/a.md"
    );
}

#[test]
fn multi_edit_shows_relative_path() {
    assert_eq!(
        summarize("MultiEdit", json!({"file_path": "/project/src/a.rs"})),
        "src/a.rs"
    );
}

#[test]
fn notebook_edit_uses_notebook_path() {
    assert_eq!(
        summarize("NotebookEdit", json!({"notebook_path": "/project/a.ipynb"})),
        "a.ipynb"
    );
}

#[test]
fn bash_shows_only_first_command_line() {
    assert_eq!(
        summarize("Bash", json!({"command": "echo first\necho second"})),
        "echo first"
    );
}

#[test]
fn grep_shows_pattern_and_relative_search_path() {
    assert_eq!(
        summarize("Grep", json!({"pattern": "fn .*", "path": "/project/src"})),
        "fn .* in src"
    );
}

#[test]
fn glob_shows_pattern_and_relative_search_path() {
    assert_eq!(
        summarize(
            "Glob",
            json!({"pattern": "**/*.rs", "path": "/project/src"})
        ),
        "**/*.rs in src"
    );
}

#[test]
fn search_without_path_shows_only_pattern() {
    assert_eq!(summarize("Glob", json!({"pattern": "**/*.rs"})), "**/*.rs");
}

#[test]
fn agent_shows_description() {
    assert_eq!(
        summarize("Agent", json!({"description": "Review parser"})),
        "Review parser"
    );
}

#[test]
fn task_shows_description() {
    assert_eq!(
        summarize("Task", json!({"description": "Review mapper"})),
        "Review mapper"
    );
}

#[test]
fn web_fetch_shows_url() {
    assert_eq!(
        summarize("WebFetch", json!({"url": "https://example.com/a"})),
        "https://example.com/a"
    );
}

#[test]
fn web_search_shows_query() {
    assert_eq!(
        summarize("WebSearch", json!({"query": "Rust docs"})),
        "Rust docs"
    );
}

#[test]
fn unknown_tool_shows_name_and_first_input_value() {
    assert_eq!(
        summarize(
            "Custom",
            json!({"argument": "details", "z_other": "ignored"})
        ),
        "Custom details"
    );
}

#[test]
fn unknown_tool_renders_non_string_value_as_json() {
    assert_eq!(
        summarize("Custom", json!({"argument": [1, true]})),
        "Custom [1,true]"
    );
}

#[test]
fn unknown_tool_without_fields_shows_name() {
    assert_eq!(summarize("Custom", json!({})), "Custom");
}

#[test]
fn missing_or_invalid_known_fields_are_empty() {
    assert_eq!(summarize("Read", json!({"file_path": 7})), "");
}

#[test]
fn path_outside_project_keeps_absolute_path() {
    assert_eq!(
        summarize("Edit", json!({"file_path": "/project-other/a"})),
        "/project-other/a"
    );
}

#[test]
fn project_root_is_displayed_as_dot() {
    assert_eq!(
        summarize("Grep", json!({"pattern": "test", "path": "/project"})),
        "test in ."
    );
}

#[test]
fn summary_is_cut_at_two_hundred_characters() {
    let input = json!({"description": "é".repeat(201)});

    assert_eq!(summarize("Agent", input), "é".repeat(200));
}

#[test]
fn patch_counts_additions_and_removals_across_hunks() {
    assert_eq!(patch_counts(&structured_patch()), (3, 1));
}

#[test]
fn patch_counts_ignore_invalid_lines_and_hunks() {
    let patch = json!([null, {"lines": [null, 5, " context", "", "+", "-", "\\ No newline"]}]);

    assert_eq!(patch_counts(&patch), (1, 1));
}

#[test]
fn unified_patch_renders_headers_and_lines() {
    assert_eq!(
        unified_patch(&structured_patch()),
        "@@ -40,7 +40,10 @@\n ctx\n-old\n+new\n@@ -60,0 +63,2 @@\n+first\n+second\n"
    );
}

#[test]
fn unified_patch_ignores_invalid_lines_and_hunks() {
    let patch = json!([null, {"oldStart": 1, "oldLines": 0, "newStart": 1, "newLines": 1,
                             "lines": [null, "+new"]}]);

    assert_eq!(unified_patch(&patch), "@@ -1,0 +1,1 @@\n+new\n");
}

#[test]
fn patch_helpers_accept_missing_patch() {
    assert_eq!(
        (patch_counts(&Value::Null), unified_patch(&Value::Null)),
        ((0, 0), String::new())
    );
}

#[test]
fn unified_patch_is_bounded_at_utf8_boundary() {
    let patch = json!([{"oldStart": 1, "oldLines": 0, "newStart": 1, "newLines": 1,
                        "lines": [format!("+{}", "é".repeat(40_000)), "+later"]}]);
    let expected = format!("@@ -1,0 +1,1 @@\n+{}", "é".repeat(32_759));

    assert_eq!(unified_patch(&patch), expected);
}

#[test]
fn result_text_preserves_string() {
    assert_eq!(
        result_text(&json!("output\nnext")),
        ("output\nnext".to_owned(), false)
    );
}

#[test]
fn result_text_concatenates_only_text_blocks() {
    let content = json!([
        {"type": "text", "text": "one\n"},
        {"type": "image", "text": "ignored"},
        {"type": "text", "text": "two"},
        {"type": "text", "text": null}, null
    ]);

    assert_eq!(result_text(&content), ("one\ntwo".to_owned(), false));
}

#[test]
fn result_text_ignores_unsupported_content() {
    assert_eq!(
        result_text(&json!({"text": "ignored"})),
        (String::new(), false)
    );
}

#[test]
fn result_text_exact_limit_is_not_truncated() {
    let text = "x".repeat(65_536);

    assert_eq!(result_text(&json!(text)), (text, false));
}

#[test]
fn result_text_truncates_on_utf8_boundary() {
    let text = format!("{}éz", "x".repeat(65_535));

    assert_eq!(result_text(&json!(text)), ("x".repeat(65_535), true));
}

#[test]
fn result_text_caps_combined_blocks_and_stops_at_first_cut() {
    let content = json!([
        {"type": "text", "text": "x".repeat(65_535)},
        {"type": "text", "text": "é"},
        {"type": "text", "text": "z"}
    ]);

    assert_eq!(result_text(&content), ("x".repeat(65_535), true));
}
