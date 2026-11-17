use super::*;
use serde_json::{json, Value};

#[test]
fn every_event_variant_round_trips_through_json() {
    let events = vec![
        NativeEvent::Admitted,
        NativeEvent::BoundaryCreated,
        NativeEvent::ThreadStarted {
            thread_id: "t".into(),
        },
        NativeEvent::TurnStarted {
            turn_id: "u".into(),
        },
        NativeEvent::TurnCompleted {
            turn_id: "u".into(),
            status: "completed".into(),
        },
        NativeEvent::Message {
            message_id: "m".into(),
            state: MessageState::Delivered,
            turn_id: Some("u".into()),
            detail: None,
        },
        NativeEvent::ServerRequestRefused { method: "m".into() },
        NativeEvent::CancelRequested { reason: "r".into() },
        NativeEvent::DeadlineExpired,
        NativeEvent::ReportStored {
            bytes: 2,
            sha256: "hash".into(),
            complete: true,
        },
        NativeEvent::Progress,
        NativeEvent::Terminal {
            outcome: Outcome::Succeeded,
            reason: "done".into(),
        },
        NativeEvent::OwnershipReleased {
            by: "supervisor".into(),
        },
        NativeEvent::OwnershipUnverified { detail: "d".into() },
    ];
    for event in events {
        let encoded = serde_json::to_value(&event).unwrap();
        assert_eq!(
            serde_json::from_value::<NativeEvent>(encoded).unwrap(),
            event
        );
    }
}

#[test]
fn event_kind_tags_are_snake_case() {
    assert_eq!(
        serde_json::to_value(NativeEvent::Admitted).unwrap(),
        json!({"kind":"admitted"})
    );
    assert_eq!(
        serde_json::to_value(NativeEvent::ServerRequestRefused { method: "x".into() }).unwrap(),
        json!({"kind":"server_request_refused","method":"x"})
    );
}

#[test]
fn enums_serialize_snake_case() {
    assert_eq!(
        serde_json::to_value(Outcome::TimedOut).unwrap(),
        json!("timed_out")
    );
    assert_eq!(
        serde_json::to_value(MessageState::CancelledBeforeDelivery).unwrap(),
        json!("cancelled_before_delivery")
    );
    assert_eq!(
        serde_json::to_value(Delivery::NextTurn).unwrap(),
        json!("next_turn")
    );
}

#[test]
fn event_record_rejects_unknown_field() {
    let record =
        json!({"seq":1,"at":"2026-01-01T00:00:00Z","event":{"kind":"admitted"},"extra":true});
    assert!(serde_json::from_value::<EventRecord>(record).is_err());
    assert!(serde_json::from_value::<NativeEvent>(
        json!({"kind":"thread_started","thread_id":"t","extra":true})
    )
    .is_err());
}

#[test]
fn manifest_rejects_unknown_field() {
    let mut manifest = json!({"v":1,"stage_id":"s","unit_id":"u","invocation_id":"i", "owner_session_id":"o", "workspace_root":"/w", "brief_path":"/b", "brief_sha256":"h", "model":"m", "effort":"e", "deadline_secs":1, "created_at":"2026-01-01T00:00:00Z"});
    assert!(serde_json::from_value::<NativeManifest>(manifest.clone()).is_ok());
    manifest
        .as_object_mut()
        .unwrap()
        .insert("extra".into(), Value::Bool(true));
    assert!(serde_json::from_value::<NativeManifest>(manifest).is_err());
}

#[test]
fn phase_orders_by_declaration() {
    assert!(Phase::Admitted < Phase::Starting && Phase::Starting < Phase::Running);
    assert!(Phase::Running < Phase::Finalizing && Phase::Finalizing < Phase::Terminal);
}
