//! Contracts for prompts Claude Code records inside a `<pasted_content id="..">` wrapper: the
//! wrapper is removed before the attachment block, the sent item and the closing origin are
//! matched.

use serde_json::json;

use super::tests::{mapper, only_turn, user};
use crate::plan_session::attachments::{parse_message, Attachment, AttachmentKind};
use crate::plan_session::model::{Delivery, Item, Origin};

const OPENING: &str = "loom plan: no step chosen.\nAsk me what I want to work on.";
const CLOSING: &str = "Thanks, that is all.";

/// The text inside the wrapper shape captured from a real transcript: two leading blank
/// lines, the open tag, the body, the close tag repeating the id, a trailing newline.
fn wrapped(id: &str, body: &str) -> String {
    format!("\n\n<pasted_content id=\"{id}\">\n{body}\n</pasted_content id=\"{id}\">\n")
}

fn record(uuid: &str, content: &str) -> String {
    json!({"type": "user", "uuid": uuid, "timestamp": "2026-11-02T04:29:00Z",
        "message": {"role": "user", "content": content},
        "origin": {"kind": "human"}})
    .to_string()
}

#[test]
fn wrapped_message_parses_typed_text_and_attachments() {
    let delivered = "<pasted_content id=\"733e\">\n\
        What are the main decisions here?\n\
        \n\
        <loom-attachments>\n\
        [1] source doc/product/feat-planner/index.md\n\
        > # Loom Plan — Product Vision\n\
        </loom-attachments>\n\
        </pasted_content id=\"733e\">";
    let (text, attachments) = parse_message(delivered);
    let expected = Attachment {
        kind: AttachmentKind::Source,
        path: "doc/product/feat-planner/index.md".into(),
        anchor: None,
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: "# Loom Plan — Product Vision".into(),
        changed_since_selected: false,
        locked: None,
        file: None,
    };
    assert_eq!(text, "What are the main decisions here?");
    assert_eq!(attachments, [expected]);
}

#[test]
fn wrapped_opening_replaces_the_sent_item() {
    let sent = Item::User {
        id: "s-1".into(),
        text: OPENING.into(),
        attachments: Vec::new(),
        delivery: Delivery::Sent,
        origin: Some(Origin::Offer),
    };
    let mut mapper = mapper();
    mapper.add_server_item(sent);
    mapper.ingest_main(&record("u1", &wrapped("f3a0", OPENING)));
    let users: Vec<&Item> = only_turn(&mapper)
        .items
        .iter()
        .filter(|item| matches!(item, Item::User { .. }))
        .collect();
    let delivered = user("u-u1", OPENING, Some(Origin::Offer));
    assert_eq!(users, [&delivered], "the opening shows once, unwrapped");
}

#[test]
fn wrapped_closing_keeps_its_origin() {
    let mut mapper = mapper();
    mapper.expect_closing(CLOSING.into());
    mapper.ingest_main(&record("u9", &wrapped("9b1c", CLOSING)));
    let expected = user("u-u9", CLOSING, Some(Origin::Closing));
    assert_eq!(only_turn(&mapper).items, [expected]);
}
