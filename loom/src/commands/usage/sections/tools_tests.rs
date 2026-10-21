use std::collections::BTreeMap;

use super::*;
use crate::commands::usage::transcript::{
    Request, RequestNormalization, TokenUsage, ToolUse, UserEntry,
};

fn request(tool_uses: Vec<ToolUse>) -> Entry {
    Entry::Assistant(Box::new(Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: "claude-sonnet-5".to_owned(),
        usage: TokenUsage::default(),
        tool_uses,
        thinking_chars: 0,
        text_chars: 0,
        normalization: RequestNormalization::default(),
    }))
}

fn tool(id: &str, name: &str, command: &str) -> ToolUse {
    ToolUse {
        id: id.to_owned(),
        name: name.to_owned(),
        input: serde_json::json!({ "command": command }),
    }
}

fn result(tool_use_id: &str, text: &str) -> Entry {
    Entry::User(UserEntry {
        timestamp: chrono::Utc::now(),
        tool_use_id: Some(tool_use_id.to_owned()),
        text: text.to_owned(),
    })
}

#[test]
fn cargo_test_commands_share_a_key() {
    assert_eq!(command_key("cargo test --lib x"), "cargo test");
    assert_eq!(command_key("cargo test"), "cargo test");
}

#[test]
fn command_key_drops_environment_assignments() {
    assert_eq!(command_key("RUST_LOG=debug cargo build"), "cargo build");
}

#[test]
fn command_key_drops_a_leading_directory_change() {
    assert_eq!(command_key("cd loom && cargo fmt"), "cargo fmt");
}

#[test]
fn command_key_uses_the_program_basename() {
    assert_eq!(command_key("/usr/bin/rg -n foo"), "rg");
}

#[test]
fn command_key_marks_empty_commands() {
    assert_eq!(command_key("   "), "(empty)");
}

#[test]
fn add_entries_counts_bash_result_bytes() {
    let entries = vec![
        request(vec![tool("bash-1", "Bash", "cargo test --lib x")]),
        result("bash-1", "λ"),
        result("bash-1", "output"),
    ];
    let mut totals = BTreeMap::new();

    add_entries(&mut totals, &entries);

    assert_eq!(totals.get("cargo test"), Some(&(2, 8, 6)));
}

#[test]
fn add_entries_ignores_non_bash_results() {
    let entries = vec![
        request(vec![tool("read-1", "Read", "ignored")]),
        result("read-1", "output"),
    ];
    let mut totals = BTreeMap::new();

    add_entries(&mut totals, &entries);

    assert!(totals.is_empty());
}

#[test]
fn bash_output_limits_the_command_ranking() {
    let entries = (0..25)
        .flat_map(|index| {
            let id = format!("bash-{index}");
            let command = format!("/bin/command{index:02}");
            [
                request(vec![tool(&id, "Bash", &command)]),
                result(&id, "output"),
            ]
        })
        .collect::<Vec<_>>();
    let mut totals = BTreeMap::new();

    add_entries(&mut totals, &entries);
    let output = bash_output(totals);

    assert_eq!(output.calls, 25);
    assert_eq!(output.by_command.len(), 20);
    assert_eq!(output.by_command[0].command, "command00");
    assert_eq!(output.by_command[19].command, "command19");
}

#[test]
fn a_command_key_with_an_escape_byte_prints_without_it() {
    let key = command_key("\u{1b}]52;c;AAAA\u{7}evil arg");
    assert!(key.contains('\u{1b}'));
    let row = render_row(&CommandOutput {
        command: key,
        calls: 1,
        output_bytes: 2,
        max_bytes: 2,
    });
    assert!(!row.contains('\u{1b}'));
    assert!(!row.contains('\u{7}'));
}

#[test]
fn a_huge_single_word_yields_a_bounded_key() {
    let key = command_key(&"x".repeat(100_000));
    assert_eq!(key.chars().count(), MAX_KEY_CHARS);
    let wide = command_key(&"é".repeat(100_000));
    assert_eq!(wide.chars().count(), MAX_KEY_CHARS);
}
