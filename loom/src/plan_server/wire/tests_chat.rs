//! Parity tests of the chat wire types against the web fixtures.

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use super::chat::{ChatCommand, ChatFrame};
use super::message::ClientMessage;
use crate::plan_session::attachments::{AttachmentInput, AttachmentKind, LockedTag};
use crate::plan_session::model::{
    ChatDelta, ChatSnapshot, Delivery, DenyReason, FailureKind, Item, NoticeKind, Origin,
    PermissionStatus, SessionState, ToolStatus,
};

const SNAPSHOT: &str = include_str!("../../../../web/src/plan/api/fixtures/chat-snapshot.json");
const DELTAS: &str = include_str!("../../../../web/src/plan/api/fixtures/chat-deltas.json");
const COMMANDS: &str = include_str!("../../../../web/src/plan/api/fixtures/chat-commands.json");
const FRAMES: &str = include_str!("../../../../web/src/plan/api/fixtures/chat-frames.json");

fn assert_parity<T: DeserializeOwned + Serialize>(fixture: &str) {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let parsed: T = serde_json::from_str(fixture).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
}

macro_rules! fixture_tests {
    ($parity:ident, $deserializes:ident, $fixture:ident, $ty:ty) => {
        #[test]
        fn $parity() {
            assert_parity::<$ty>($fixture);
        }
        #[test]
        fn $deserializes() {
            serde_json::from_str::<$ty>($fixture).unwrap();
        }
    };
}

fixture_tests!(
    chat_snapshot_fixture_matches_serde_output,
    chat_snapshot_fixture_deserializes,
    SNAPSHOT,
    ChatSnapshot
);
fixture_tests!(
    chat_deltas_fixture_matches_serde_output,
    chat_deltas_fixture_deserializes,
    DELTAS,
    Vec<ChatDelta>
);
fixture_tests!(
    chat_commands_fixture_matches_serde_output,
    chat_commands_fixture_deserializes,
    COMMANDS,
    Vec<ChatCommand>
);
fixture_tests!(
    chat_frames_fixture_matches_serde_output,
    chat_frames_fixture_deserializes,
    FRAMES,
    Vec<ChatFrame>
);

// Search only the specified field, optionally restricting the containing object by its tag.
fn contains_field(value: &Value, field: &str, tag: Option<(&str, &str)>, wanted: &Value) -> bool {
    match value {
        Value::Array(values) => values.iter().any(|v| contains_field(v, field, tag, wanted)),
        Value::Object(map) => {
            let matches_tag =
                tag.is_none_or(|(key, value)| map.get(key).and_then(Value::as_str) == Some(value));
            (matches_tag && map.get(field) == Some(wanted))
                || map.values().any(|v| contains_field(v, field, tag, wanted))
        }
        _ => false,
    }
}

fn assert_variants<T: Serialize + std::fmt::Debug>(
    values: &[T],
    field: &str,
    tag: Option<(&str, &str)>,
) {
    let fixtures: Vec<Value> = [SNAPSHOT, DELTAS, COMMANDS, FRAMES]
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let missing: Vec<_> = values
        .iter()
        .filter(|variant| {
            let wire = serde_json::to_value(variant).unwrap();
            !fixtures
                .iter()
                .any(|fixture| contains_field(fixture, field, tag, &wire))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "fixtures missing {field} variants: {missing:?}"
    );
}

// Exhaustive matches make new variants require an explicit fixture coverage case.
macro_rules! scalar_coverage {
    ($test:ident, $ty:ident, $field:literal, $tag:expr, [$($variant:ident),+ $(,)?]) => {
        #[test]
        fn $test() {
            let values = [$($ty::$variant),+];
            for value in &values { match value { $($ty::$variant)|+ => {} } }
            assert_variants(&values, $field, $tag);
        }
    };
}

scalar_coverage!(
    session_state_variants_in_fixtures,
    SessionState,
    "state",
    None,
    [Starting, Idle, Working, Permission, Ending, Ended, Failed]
);
scalar_coverage!(
    failure_kind_variants_in_fixtures,
    FailureKind,
    "kind",
    None,
    [Spawn, Died]
);
scalar_coverage!(
    delivery_variants_in_fixtures,
    Delivery,
    "delivery",
    Some(("type", "user")),
    [Sent, Delivered, Undelivered]
);
scalar_coverage!(
    origin_variants_in_fixtures,
    Origin,
    "origin",
    Some(("type", "user")),
    [Offer, Start, Closing]
);
scalar_coverage!(
    tool_status_variants_in_fixtures,
    ToolStatus,
    "status",
    Some(("type", "tool")),
    [Pending, Done, Error, Denied, NotFinished]
);
scalar_coverage!(
    permission_status_variants_in_fixtures,
    PermissionStatus,
    "status",
    Some(("type", "permission")),
    [Pending, Allowed, Denied]
);
scalar_coverage!(
    deny_reason_variants_in_fixtures,
    DenyReason,
    "deny_reason",
    Some(("type", "permission")),
    [
        Holder,
        Timeout,
        SessionEnded,
        ServerStopping,
        HookError,
        Interrupt,
        SummarySession
    ]
);
scalar_coverage!(
    notice_kind_variants_in_fixtures,
    NoticeKind,
    "kind",
    Some(("type", "notice")),
    [Interrupted, Compacted, ApiError, Stopped, Info]
);
scalar_coverage!(
    attachment_kind_variants_in_fixtures,
    AttachmentKind,
    "kind",
    None,
    [Source, Summary, File]
);
scalar_coverage!(
    locked_tag_variants_in_fixtures,
    LockedTag,
    "locked",
    None,
    [Running, Done, Retired, Knowledge, Brief]
);

fn assert_tags(fixture: &str, field: &str, expected: &[&str]) {
    let value: Value = serde_json::from_str(fixture).unwrap();
    let missing: Vec<_> = expected
        .iter()
        .filter(|tag| !contains_field(&value, field, None, &Value::String(tag.to_string())))
        .collect();
    assert!(
        missing.is_empty(),
        "fixture missing {field} variants: {missing:?}"
    );
}

#[test]
fn chat_frame_variants_in_fixtures() {
    for frame in serde_json::from_str::<Vec<ChatFrame>>(FRAMES).unwrap() {
        match frame {
            ChatFrame::Snapshot(_)
            | ChatFrame::Delta(_)
            | ChatFrame::Result(_)
            | ChatFrame::QueueReturned { .. } => {}
        }
    }
    assert_tags(
        FRAMES,
        "frame",
        &["snapshot", "delta", "result", "queue_returned"],
    );
}

#[test]
fn chat_command_variants_in_fixtures() {
    for command in serde_json::from_str::<Vec<ChatCommand>>(COMMANDS).unwrap() {
        match command {
            ChatCommand::Start { .. }
            | ChatCommand::Send { .. }
            | ChatCommand::ConfirmKickoff { .. }
            | ChatCommand::Withdraw { .. }
            | ChatCommand::Permission { .. }
            | ChatCommand::Interrupt { .. }
            | ChatCommand::End { .. } => {}
        }
    }
    assert_tags(
        COMMANDS,
        "command",
        &[
            "start",
            "send",
            "withdraw",
            "permission",
            "interrupt",
            "end",
        ],
    );
}

#[test]
fn chat_delta_variants_in_fixtures() {
    for delta in serde_json::from_str::<Vec<ChatDelta>>(DELTAS).unwrap() {
        match delta {
            ChatDelta::Session { .. }
            | ChatDelta::TurnAdded { .. }
            | ChatDelta::TurnUpdated { .. }
            | ChatDelta::ItemAdded { .. }
            | ChatDelta::ItemReplaced { .. }
            | ChatDelta::ItemRemoved { .. }
            | ChatDelta::Queue { .. }
            | ChatDelta::Cleared => {}
        }
    }
    assert_tags(
        DELTAS,
        "op",
        &[
            "session",
            "turn_added",
            "turn_updated",
            "item_added",
            "item_replaced",
            "item_removed",
            "queue",
            "cleared",
        ],
    );
}

#[test]
fn item_variants_in_fixtures() {
    let snapshot: ChatSnapshot = serde_json::from_str(SNAPSHOT).unwrap();
    for item in snapshot.turns.iter().flat_map(|turn| &turn.items) {
        match item {
            Item::User { .. }
            | Item::Thinking { .. }
            | Item::Text { .. }
            | Item::Tool { .. }
            | Item::Permission { .. }
            | Item::Notice { .. } => {}
        }
    }
    assert_tags(
        SNAPSHOT,
        "type",
        &["user", "thinking", "text", "tool", "permission", "notice"],
    );
}

#[test]
fn a_chat_command_with_a_misspelt_field_is_refused() {
    let message = |field: &str| {
        let mut command = serde_json::json!({"type": "chat", "command": "permission",
            "request_id": "r1", "allow": false, "client_id": null});
        command[field] = Value::from("ask first");
        serde_json::from_value::<ClientMessage>(command)
    };

    assert!(message("notes").is_err());
    let parsed = message("note").unwrap();
    assert_eq!(
        parsed,
        ClientMessage::Chat(ChatCommand::Permission {
            client_id: None,
            request_id: "r1".into(),
            allow: false,
            note: Some("ask first".into()),
        })
    );
}

#[test]
fn a_start_whose_attachment_has_a_misspelt_field_is_refused() {
    let message = |field: &str| {
        let mut attachment = serde_json::json!({"kind": "summary", "path": "doc/x.md",
            "anchor": "s-1", "lines": null, "stage": null, "summary_sentence": "s-1",
            "rests_on": [], "quote": "A sentence."});
        attachment[field] = Value::from("sha256:ab");
        let command = serde_json::json!({"type": "chat", "command": "start", "client_id": null,
            "offer": null, "text": null, "attachments": [attachment]});
        serde_json::from_value::<ClientMessage>(command)
    };

    assert!(message("selectedHash").is_err());
    let parsed = message("selected_hash").unwrap();
    let ClientMessage::Chat(ChatCommand::Start { attachments, .. }) = parsed else {
        panic!("not a start: {parsed:?}");
    };
    let hashes: Vec<Option<&str>> = attachments
        .iter()
        .map(|attachment| match attachment {
            AttachmentInput::Document(document) => document.selected_hash.as_deref(),
            AttachmentInput::File(_) => None,
        })
        .collect();
    assert_eq!(hashes, [Some("sha256:ab")]);
}

#[test]
fn chat_start_round_trips_with_or_without_a_feature() {
    for feature in [None, Some("demo")] {
        let mut value = serde_json::json!({
            "type": "chat", "command": "start", "client_id": null,
            "offer": "write-section:01", "text": null, "attachments": []
        });
        if let Some(name) = feature {
            value["feature"] = Value::from(name);
        }

        let parsed: ClientMessage = serde_json::from_value(value.clone()).unwrap();

        let ClientMessage::Chat(ChatCommand::Start {
            feature: selected, ..
        }) = &parsed
        else {
            panic!("not a start: {parsed:?}");
        };
        assert_eq!(selected.as_deref(), feature);
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
}
