//! Contract for a kickoff opening that carries the operator's typed text: the bridge sends the
//! guidance and the typed sentence as two items, Claude Code records both inside one
//! `<pasted_content>` wrapper with a blank line between, and the mapper shows each once.

use serde_json::json;

use super::tests::mapper;
use crate::plan_session::model::{Delivery, Item, Origin};

const GUIDANCE: &str = "loom plan: Kickoff.\n\
Discuss the work and confirm its size and name before writing anything under doc/.\n\
Load the skill loom-plan:kickoff, then begin by telling me what you read and what you plan to ask.";
const TYPED: &str = "i would like to add support for microvm test environment";

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

#[test]
fn a_kickoff_opening_shows_guidance_and_typed_text_once() {
    let mut mapper = mapper();
    mapper.add_server_item(sent("s-1", GUIDANCE, Some(Origin::Offer)));
    mapper.add_server_item(sent("s-2", TYPED, None));
    mapper.expect_opening(Some(Origin::Offer));
    let content = format!(
        "\n\n<pasted_content id=\"f509\">\n{GUIDANCE}\n\n{TYPED}\n</pasted_content id=\"f509\">\n"
    );
    mapper.ingest_main(&record("u1", &content));
    let users: Vec<(&str, &str, Option<Origin>, &Delivery)> = mapper
        .turns()
        .iter()
        .flat_map(|turn| turn.items.iter())
        .filter_map(|item| match item {
            Item::User {
                id,
                text,
                origin,
                delivery,
                ..
            } => Some((id.as_str(), text.as_str(), *origin, delivery)),
            _ => None,
        })
        .collect();
    let guidance: Vec<_> = users
        .iter()
        .filter(|u| u.2 == Some(Origin::Offer))
        .collect();
    let typed: Vec<_> = users.iter().filter(|u| u.2.is_none()).collect();
    assert_eq!(guidance.len(), 1, "one guidance item: {users:?}");
    assert_eq!(typed.len(), 1, "one typed item: {users:?}");
    assert_eq!(guidance[0].1, GUIDANCE, "{users:?}");
    assert_eq!(typed[0].1, TYPED, "{users:?}");
    assert_ne!(guidance[0].0, typed[0].0, "{users:?}");
    assert!(
        users.iter().all(|u| *u.3 != Delivery::Sent),
        "no item stays sent: {users:?}"
    );
}
