use super::{task_text, WORKER_PREAMBLE};
use std::path::Path;

fn heredoc_lines() -> Vec<&'static str> {
    let script = include_str!("../../../loom-hooks/codex-forward.sh");
    let mut lines = script.split('\n');
    assert!(lines.any(|line| line.trim() == "cat <<'CODEX_PREAMBLE'"));
    lines.take_while(|line| *line != "CODEX_PREAMBLE").collect()
}

#[test]
fn preamble_matches_the_forwarder_heredoc_byte_for_byte() {
    assert_eq!(heredoc_lines().join("\n"), WORKER_PREAMBLE);
}

#[test]
fn heredoc_extraction_is_not_empty() {
    assert!(heredoc_lines().len() > 40);
}

#[test]
fn task_text_layout_is_preamble_blank_line_marker_blank_line_brief() {
    assert_eq!(task_text(Path::new("brief.md"), "do work"), format!(
        "{}\n\nYour brief follows. It was read from brief.md; relative paths in it resolve from that file's directory.\n\ndo work",
        WORKER_PREAMBLE
    ));
}

#[test]
fn task_text_keeps_braces_in_the_brief() {
    assert!(task_text(Path::new("brief.md"), "{a} {{b}} {}").ends_with("\n\n{a} {{b}} {}"));
}

#[test]
fn task_text_names_the_brief_path() {
    assert!(task_text(Path::new("some/path.md"), "brief").contains("read from some/path.md;"));
}

#[test]
fn preamble_ends_with_task_marker_and_no_newline() {
    assert!(WORKER_PREAMBLE.ends_with("=== TASK ==="));
}
