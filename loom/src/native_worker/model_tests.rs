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

const LINUX_IDENTITY: &str = r#"{"supervisor_pid":7,"supervisor_start":"11","boundary_pid":8,"pid_namespace":4026531836,"boot_id":"boot-a"}"#;

#[test]
fn a_linux_identity_round_trips_byte_for_byte_without_a_mac_part() {
    let identity: BoundaryIdentity = serde_json::from_str(LINUX_IDENTITY).unwrap();
    assert_eq!(identity.mac, None);
    assert_eq!(serde_json::to_string(&identity).unwrap(), LINUX_IDENTITY);
}

fn mac_identity() -> MacIdentity {
    MacIdentity {
        invocation: "inv-00112233445566778899aabbccddeeff".into(),
        session: 4242,
        leader_start: "1700000000123456".into(),
        boot_id: "boot-b".into(),
        scratch_root: PathBuf::from("/cache/native/inv-00112233445566778899aabbccddeeff"),
    }
}

#[test]
fn a_macos_identity_round_trips_and_marks_its_session_group_and_marker() {
    let identity = BoundaryIdentity {
        supervisor_pid: 7,
        supervisor_start: "11".into(),
        boundary_pid: 4242,
        pid_namespace: 0,
        boot_id: "boot-b".into(),
        mac: Some(mac_identity()),
    };
    let text = serde_json::to_string(&identity).unwrap();
    assert!(text.contains(r#""mac":{"invocation":"#), "{text}");
    assert_eq!(
        serde_json::from_str::<BoundaryIdentity>(&text).unwrap(),
        identity
    );
    let mark = mac_identity().mark();
    assert_eq!((mark.sid, mark.pgid), (4242, 4242));
    assert_eq!(
        mark.marker,
        "LOOM_NATIVE_BOUNDARY=inv-00112233445566778899aabbccddeeff"
    );
}

#[test]
fn a_macos_identity_rejects_unknown_fields() {
    let mut value = serde_json::to_value(mac_identity()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("extra".into(), Value::Bool(true));
    assert!(serde_json::from_value::<MacIdentity>(value).is_err());
}

#[test]
fn phase_orders_by_declaration() {
    assert!(Phase::Admitted < Phase::Starting && Phase::Starting < Phase::Running);
    assert!(Phase::Running < Phase::Finalizing && Phase::Finalizing < Phase::Terminal);
}
