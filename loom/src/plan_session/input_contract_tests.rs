//! Contract: the send gate classes a slash message by its first token after it strips controls,
//! leading whitespace and U+FEFF, and finds a command by its name or any of its aliases.

use serde_json::Value;

use super::commands::CommandTable;
use super::input::{check_send, InputRefusal};

const INITIALIZE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
));

/// A Ready table over the rows of the recorded `initialize` answer.
fn ready_table() -> CommandTable {
    let first = INITIALIZE
        .lines()
        .next()
        .expect("the initialize answer line");
    let answer: Value = serde_json::from_str(first).expect("the answer is JSON");
    let rows = answer["response"]["response"]["commands"].as_array();
    CommandTable::from_initialize(rows.expect("the answer's command rows"), 1)
}

fn refused(reason: &str) -> Result<(), InputRefusal> {
    Err(InputRefusal::Command(reason.to_owned()))
}

#[test]
fn disguised_and_aliased_terminal_commands_are_refused() {
    let ready = ready_table();
    let cases = [
        ("\u{1}/clear", "Changes the session outside the planner"),
        ("\u{feff}/exit", "Changes the session outside the planner"),
        ("\u{85}/clear", "Changes the session outside the planner"),
        ("/cost", "Opens a terminal dialog"),
        ("/stats", "Opens a terminal dialog"),
        ("/effort high", "Saves your default for new sessions"),
        ("/compactx", "Not in this session's command list"),
    ];
    for (text, reason) in cases {
        assert_eq!(check_send(text, &ready), refused(reason), "{text:?}");
    }
    for text in ["/simplify", "/loom-plan:args x"] {
        assert_eq!(check_send(text, &ready), Ok(()), "{text:?}");
    }

    let loading = CommandTable::loading();
    let still_loading = refused("Commands are still loading");
    assert_eq!(check_send("/simplify", &loading), still_loading);
    assert_eq!(check_send("plain text", &loading), Ok(()));
    let shown = InputRefusal::Command("Opens a terminal dialog".to_owned()).to_string();
    assert_eq!(shown, "Opens a terminal dialog");
}
