//! The close reason's own treatment in the status payload: whole, with its
//! lines, bounded at `MAX_CLOSE_REASON_CHARS`.
use super::tests::summary;
use super::*;
use crate::context::untrusted::MAX_INLINE_CHARS;

#[test]
fn a_control_sequence_survives_the_close_reason_path_only_neutralized() {
    let mut stage = summary();
    stage.close_reason =
        Some("first line\u{1b}]52;c;ZXZpbA==\u{7}\r\nsecond\u{202E}line\n\n\n\nlast".to_string());

    sanitize_stage_summary(&mut stage);

    // Line breaks stay, the controls become spaces (trailing ones trimmed), and
    // a flood of blank lines is squeezed to one.
    assert_eq!(
        stage.close_reason.as_deref(),
        Some("first line ]52;c;ZXZpbA==\nsecond line\n\nlast")
    );
}

#[test]
fn a_long_close_reason_is_kept_whole_with_its_lines() {
    let mut stage = summary();
    let reason = "Criterion 5 cannot pass in this sandbox: it denies AF_UNIX socket creation \
                  everywhere, and the filer always opens one.\nNeeded: run criterion 5 with \
                  unix sockets allowed or outside the sandbox."
        .repeat(3);
    assert!(reason.chars().count() > MAX_INLINE_CHARS * 2);
    stage.close_reason = Some(reason.clone());

    sanitize_stage_summary(&mut stage);

    assert_eq!(stage.close_reason, Some(reason));
}

#[test]
fn an_over_long_close_reason_is_cut_and_says_where_the_rest_is() {
    let mut stage = summary();
    stage.close_reason = Some(format!(
        "{}\n{}",
        "é".repeat(MAX_CLOSE_REASON_CHARS),
        "tail"
    ));

    sanitize_stage_summary(&mut stage);

    let reason = stage.close_reason.unwrap();
    let (kept, marker) = reason.rsplit_once('\n').unwrap();
    assert_eq!(kept.chars().count(), MAX_CLOSE_REASON_CHARS);
    assert_eq!(marker, CLOSE_REASON_TRUNCATED_MARKER);
}

#[test]
fn a_blank_close_reason_becomes_none() {
    let mut stage = summary();
    stage.close_reason = Some(" \n ".to_string());

    sanitize_stage_summary(&mut stage);

    assert_eq!(stage.close_reason, None);
}

#[test]
fn acceptance_commands_and_the_history_session_are_flattened() {
    let mut stage = summary();
    stage.acceptance = vec!["cargo test\u{1b}[2J\nand more".to_string()];
    stage.terminal_history_session = Some("session\u{202E}1".to_string());

    sanitize_stage_summary(&mut stage);

    assert_eq!(stage.acceptance, ["cargo test [2J and more"]);
    assert_eq!(stage.terminal_history_session.as_deref(), Some("session 1"));
}
