//! The mapper over prompt records beyond the plain case: a delivered opening that carries
//! attachments, a sent item the hook already confirmed, and the skill body Claude Code records
//! after a Skill tool call.

use serde_json::json;

use super::tests::mapper;
use crate::plan_session::model::{Delivery, Item, Origin};

const GUIDANCE: &str = "loom plan: Kickoff.\nLoad the skill loom-plan:kickoff.";
const TYPED: &str = "add support for a microvm test environment";
const BLOCK: &str =
    "<loom-attachments>\n[1] source doc/product/demo/index.md\n> # Vision\n</loom-attachments>";

fn sent(id: &str, text: &str, origin: Option<Origin>) -> Item {
    Item::User {
        id: id.into(),
        text: text.into(),
        attachments: Vec::new(),
        delivery: Delivery::Sent,
        origin,
        command: None,
    }
}

fn record(uuid: &str, content: &str) -> String {
    json!({"type": "user", "uuid": uuid, "timestamp": "2026-11-02T04:29:00Z",
        "message": {"role": "user", "content": content},
        "origin": {"kind": "human"}})
    .to_string()
}

fn users(mapper: &super::Mapper) -> Vec<&Item> {
    let items = mapper.turns().iter().flat_map(|turn| turn.items.iter());
    items
        .filter(|item| matches!(item, Item::User { .. }))
        .collect()
}

#[test]
fn a_meta_skill_body_with_list_content_is_skipped() {
    let body = "Base directory for this skill: /runtime/plugin/skills/kickoff\n\n# Start new work";
    let line = json!({"type": "user", "uuid": "m1", "timestamp": "2026-11-02T04:29:00Z",
        "isMeta": true, "sourceToolUseID": "toolu_01",
        "message": {"role": "user", "content": [{"type": "text", "text": body}]}})
    .to_string();
    let mut mapper = mapper();
    let ingest = mapper.ingest_main(&line);
    assert!(ingest.marks.is_empty(), "{:?}", ingest.marks);
    assert!(ingest.deltas.is_empty(), "{:?}", ingest.deltas);
    assert!(mapper.turns().is_empty(), "{:?}", mapper.turns());
}

#[test]
fn a_split_opening_gives_its_attachments_to_the_typed_item() {
    let mut mapper = mapper();
    mapper.add_server_item(sent("s-1", GUIDANCE, Some(Origin::Offer)));
    mapper.add_server_item(sent("s-2", TYPED, None));
    mapper.expect_opening(Some(Origin::Offer));
    let body = format!("{GUIDANCE}\n\n{TYPED}\n\n{BLOCK}");
    let content =
        format!("\n\n<pasted_content id=\"a1b2\">\n{body}\n</pasted_content id=\"a1b2\">\n");

    mapper.ingest_main(&record("u1", &content));

    let shown = users(&mapper);
    let [Item::User {
        text: guidance_text,
        attachments: guidance_chips,
        origin: guidance_origin,
        ..
    }, Item::User {
        text: typed_text,
        attachments: typed_chips,
        origin: typed_origin,
        ..
    }] = shown.as_slice()
    else {
        panic!("two user items expected: {shown:?}");
    };
    assert_eq!(guidance_text, GUIDANCE);
    assert_eq!(*guidance_origin, Some(Origin::Offer));
    assert!(guidance_chips.is_empty());
    assert_eq!(typed_text, TYPED);
    assert_eq!(*typed_origin, None);
    assert_eq!(typed_chips.len(), 1, "{typed_chips:?}");
}

#[test]
fn a_guidance_only_opening_stays_one_origin_item() {
    let mut mapper = mapper();
    mapper.add_server_item(sent("s-1", GUIDANCE, Some(Origin::Offer)));
    mapper.expect_opening(Some(Origin::Offer));

    mapper.ingest_main(&record("u1", GUIDANCE));

    let shown = users(&mapper);
    let [Item::User { text, origin, .. }] = shown.as_slice() else {
        panic!("one user item expected: {shown:?}");
    };
    assert_eq!(text, GUIDANCE);
    assert_eq!(*origin, Some(Origin::Offer));
}

#[test]
fn a_sent_item_the_hook_confirmed_is_still_replaced_by_its_record() {
    let text = "Make this a feature instead.";
    let mut mapper = mapper();
    mapper.add_server_item(sent("s-1", text, None));
    let confirmed = Item::User {
        id: "s-1".into(),
        text: text.into(),
        attachments: Vec::new(),
        delivery: Delivery::Delivered,
        origin: None,
        command: None,
    };
    mapper.replace_server_item(confirmed);

    mapper.ingest_main(&record("u1", text));

    let shown = users(&mapper);
    assert_eq!(shown.len(), 1, "{shown:?}");
}
