//! Contracts for prompts Claude Code records with `<pasted_content id="..">` wrappers, whole or
//! split around the text: the wrappers are removed before the attachment block, the sent item
//! and the closing origin are matched, and whitespace at the edges of a record does not matter.

use serde_json::json;

use super::records::item_id;
use super::tests::{mapper, only_turn, user};
use super::{Mapper, RecordMark};
use crate::plan_session::attachments::{format_message, parse_message, Attachment, AttachmentKind};
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
        command: None,
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

fn source() -> Attachment {
    Attachment {
        kind: AttachmentKind::Source,
        path: "doc/product/feat-planner/index.md".into(),
        anchor: None,
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: "# Loom Plan".into(),
        changed_since_selected: false,
        locked: None,
        file: None,
    }
}

fn sent_item(text: &str, attachments: Vec<Attachment>) -> Item {
    Item::User {
        id: "s-1".into(),
        text: text.into(),
        attachments,
        delivery: Delivery::Delivered,
        origin: Some(Origin::Offer),
        command: None,
    }
}

fn users(mapper: &Mapper) -> Vec<Item> {
    let items = only_turn(mapper).items.iter();
    items
        .filter(|item| matches!(item, Item::User { .. }))
        .cloned()
        .collect()
}

#[test]
fn a_paste_split_before_the_closing_tag_replaces_the_sent_item() {
    let text = "What are the main decisions here?";
    let attachments = vec![source()];
    let full = format_message(text, &attachments);
    let (head, tail) = full.rsplit_once('\n').unwrap();
    assert_eq!(tail, "</loom-attachments>");
    let recorded = format!(
        "\n\n<pasted_content id=\"5193\">\n{head}\n</pasted_content id=\"5193\">\n\n{tail}"
    );
    let mut mapper = mapper();
    mapper.add_server_item(sent_item(text, attachments.clone()));
    mapper.ingest_main(&record("u1", &recorded));
    let delivered = Item::User {
        id: "u-u1".into(),
        text: text.into(),
        attachments,
        delivery: Delivery::Delivered,
        origin: Some(Origin::Offer),
        command: None,
    };
    assert_eq!(users(&mapper), [delivered]);
}

#[test]
fn a_record_with_trimmed_edges_replaces_the_sent_item_and_keeps_its_text() {
    let sent = "  padded text\n\n";
    let mut mapper = mapper();
    mapper.add_server_item(sent_item(sent, Vec::new()));
    let out = mapper.ingest_main(&record("u1", "padded text"));
    assert_eq!(users(&mapper), [user("u-u1", sent, Some(Origin::Offer))]);
    let marked =
        matches!(&out.marks[..], [RecordMark::HumanUser { text, .. }] if text == "padded text");
    assert!(marked, "{:?}", out.marks);
}

#[test]
fn a_record_that_differs_beyond_whitespace_keeps_the_sent_item() {
    let mut mapper = mapper();
    mapper.add_server_item(sent_item("a b", Vec::new()));
    mapper.ingest_main(&record("u1", "ab"));
    let kept = users(&mapper);
    let ids: Vec<&str> = kept.iter().map(item_id).collect();
    assert_eq!(ids, ["s-1", "u-u1"]);
}
