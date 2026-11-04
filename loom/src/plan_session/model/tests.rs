//! Serde and default-value tests of the chat model types.

use super::*;
use crate::plan_session::attachments::{AttachmentKind, LockedTag};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
    let json = serde_json::to_string(value).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&json).unwrap(), value);
}

fn attachment() -> Attachment {
    Attachment {
        kind: AttachmentKind::Source,
        path: "doc/spec.md".into(),
        anchor: Some("heading".into()),
        lines: Some((1, 3)),
        stage: Some("stage".into()),
        summary_sentence: Some("s-12345678".into()),
        rests_on: vec!["doc/other.md#heading".into()],
        quote: "quoted text".into(),
        changed_since_selected: true,
        locked: Some(LockedTag::Running),
        file: None,
    }
}

fn session() -> SessionView {
    SessionView {
        id: "session".into(),
        kind: "authoring".into(),
        state: SessionState::Working,
        name: "Planning".into(),
        model: "model".into(),
        socket: "socket".into(),
        holder: Some("holder".into()),
        away_until: Some("2026-10-28T10:00:00Z".into()),
        queued: 2,
        started_at: "2026-10-28T09:00:00Z".into(),
        state_since: "2026-10-28T09:01:00Z".into(),
        working_since: Some("2026-10-28T09:02:00Z".into()),
        subagents_running: 1,
        last_activity: Some("2026-10-28T09:03:00Z".into()),
        failure: Some(Failure {
            kind: FailureKind::Died,
            message: "gone".into(),
        }),
        offer: Some("offer".into()),
        feature: Some("feature".into()),
        plan: None,
    }
}

fn tokens() -> TokenCounts {
    TokenCounts {
        input: 1,
        cache_write: 2,
        cache_read: 3,
        output: 4,
    }
}

fn changed_file() -> ChangedFile {
    ChangedFile {
        path: "doc/spec.md".into(),
        before_hash: Some("hash".into()),
        by_tool: true,
    }
}

fn queue() -> QueuedMessage {
    QueuedMessage {
        id: "q1".into(),
        first_line: "hello".into(),
        text: Some("hello\nworld".into()),
    }
}

fn user_item() -> Item {
    Item::User {
        id: "u1".into(),
        text: "hello".into(),
        attachments: vec![attachment()],
        delivery: Delivery::Delivered,
        origin: Some(Origin::Offer),
    }
}

fn tool_item() -> Item {
    Item::Tool {
        id: "a3".into(),
        tool_use_id: "tool1".into(),
        name: "Edit".into(),
        summary: "edit file".into(),
        status: ToolStatus::Done,
        subagent: Some("agent".into()),
        parent: Some("tool0".into()),
        input: "input".into(),
        result: Some("result".into()),
        result_truncated: true,
        error_line: Some("error".into()),
        added: Some(2),
        removed: Some(1),
        patch: Some("patch".into()),
        nested_tools: 3,
    }
}

fn permission_item() -> Item {
    Item::Permission {
        id: "p1".into(),
        request_id: "request".into(),
        tool: "Edit".into(),
        input: "input".into(),
        status: PermissionStatus::Denied,
        deny_reason: Some(DenyReason::Timeout),
        note: Some("expired".into()),
        expires_at: "2026-10-28T10:00:00Z".into(),
    }
}

fn items() -> Vec<(Item, &'static str)> {
    vec![
        (user_item(), "user"),
        (
            Item::Thinking {
                id: "a1".into(),
                duration_ms: Some(20),
            },
            "thinking",
        ),
        (
            Item::Text {
                id: "a2".into(),
                markdown: "**hello**".into(),
            },
            "text",
        ),
        (tool_item(), "tool"),
        (permission_item(), "permission"),
        (
            Item::Notice {
                id: "n1".into(),
                kind: NoticeKind::Interrupted,
                text: "stopped".into(),
            },
            "notice",
        ),
    ]
}

fn turn() -> Turn {
    Turn {
        id: "t1".into(),
        started_at: "2026-10-28T09:00:00Z".into(),
        duration_ms: Some(42),
        open: false,
        tokens: tokens(),
        items: items().into_iter().map(|(item, _)| item).collect(),
        changed_files: vec![changed_file()],
        summaries_rebuilding: vec!["summary".into()],
    }
}

fn deltas() -> Vec<(ChatDelta, &'static str)> {
    vec![
        (ChatDelta::Session { session: session() }, "session"),
        (ChatDelta::TurnAdded { turn: turn() }, "turn_added"),
        (
            ChatDelta::TurnUpdated {
                turn_id: "t1".into(),
                duration_ms: Some(42),
                open: false,
                tokens: tokens(),
                changed_files: vec![changed_file()],
                summaries_rebuilding: vec!["summary".into()],
            },
            "turn_updated",
        ),
        (
            ChatDelta::ItemAdded {
                turn_id: "t1".into(),
                item: items().remove(0).0,
            },
            "item_added",
        ),
        (
            ChatDelta::ItemReplaced {
                turn_id: "t1".into(),
                item: items().remove(1).0,
            },
            "item_replaced",
        ),
        (
            ChatDelta::ItemRemoved {
                turn_id: "t1".into(),
                id: "u1".into(),
            },
            "item_removed",
        ),
        (
            ChatDelta::Queue {
                queue: vec![queue()],
            },
            "queue",
        ),
        (ChatDelta::Cleared, "cleared"),
    ]
}

#[test]
fn structs_round_trip() {
    let ended = EndedChat {
        session: session(),
        turns: vec![turn()],
    };
    round_trip(&session());
    round_trip(&Failure {
        kind: FailureKind::Spawn,
        message: "failed".into(),
    });
    round_trip(&tokens());
    round_trip(&changed_file());
    round_trip(&turn());
    round_trip(&queue());
    round_trip(&ended);
    round_trip(&ChatSnapshot {
        session: Some(session()),
        turns: vec![turn()],
        queue: vec![queue()],
        previous: Some(ended),
    });
}

#[test]
fn item_variants_round_trip_with_wire_tags() {
    for (item, tag) in items() {
        round_trip(&item);
        assert_eq!(serde_json::to_value(&item).unwrap()["type"], tag);
    }
}

#[test]
fn delta_variants_round_trip_with_wire_ops() {
    for (delta, op) in deltas() {
        round_trip(&delta);
        assert_eq!(serde_json::to_value(&delta).unwrap()["op"], op);
    }
}

fn enum_values<T: Serialize + DeserializeOwned + PartialEq + Debug>(values: &[(T, &str)]) {
    for (value, expected) in values {
        round_trip(value);
        assert_eq!(serde_json::to_value(value).unwrap(), *expected);
    }
}

#[test]
fn session_enums_round_trip_as_snake_case() {
    enum_values(&[
        (SessionState::Starting, "starting"),
        (SessionState::Idle, "idle"),
        (SessionState::Working, "working"),
        (SessionState::Permission, "permission"),
        (SessionState::Ending, "ending"),
        (SessionState::Ended, "ended"),
        (SessionState::Failed, "failed"),
    ]);
    enum_values(&[
        (Activity::Idle, "idle"),
        (Activity::Working, "working"),
        (Activity::Permission, "permission"),
    ]);
    enum_values(&[(FailureKind::Spawn, "spawn"), (FailureKind::Died, "died")]);
}

#[test]
fn item_enums_round_trip_as_snake_case() {
    enum_values(&[
        (Delivery::Sent, "sent"),
        (Delivery::Delivered, "delivered"),
        (Delivery::Undelivered, "undelivered"),
    ]);
    enum_values(&[
        (Origin::Offer, "offer"),
        (Origin::Start, "start"),
        (Origin::Closing, "closing"),
    ]);
    enum_values(&[
        (ToolStatus::Pending, "pending"),
        (ToolStatus::Done, "done"),
        (ToolStatus::Error, "error"),
        (ToolStatus::Denied, "denied"),
        (ToolStatus::NotFinished, "not_finished"),
    ]);
    enum_values(&[
        (PermissionStatus::Pending, "pending"),
        (PermissionStatus::Allowed, "allowed"),
        (PermissionStatus::Denied, "denied"),
    ]);
    enum_values(&[
        (DenyReason::Holder, "holder"),
        (DenyReason::Timeout, "timeout"),
        (DenyReason::SessionEnded, "session_ended"),
        (DenyReason::ServerStopping, "server_stopping"),
        (DenyReason::HookError, "hook_error"),
        (DenyReason::Interrupt, "interrupt"),
        (DenyReason::SummarySession, "summary_session"),
    ]);
    enum_values(&[
        (NoticeKind::Interrupted, "interrupted"),
        (NoticeKind::Compacted, "compacted"),
        (NoticeKind::ApiError, "api_error"),
        (NoticeKind::Stopped, "stopped"),
        (NoticeKind::Info, "info"),
    ]);
}

#[test]
fn token_counts_default_to_zero() {
    assert_eq!(
        TokenCounts::default(),
        TokenCounts {
            input: 0,
            cache_write: 0,
            cache_read: 0,
            output: 0
        }
    );
}
