//! Worker control decoding, ownership, locking and ledger notes.

use std::path::Path;

use crate::codex::CodexBackend;
use crate::fs::stage_admission;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Implementer, Implementers, StageStatus};
use crate::native_worker::model::Delivery;
use crate::native_worker::store;
use crate::relay::{decode_payload, AgentRole, RequestKind, RequestPayload, WorkerControlPayload};
use crate::verify::transitions::update_stage;

use super::test_support::{entry_for, fixture, seed_native_invocation, Fixture, STAGE};
use super::worker_control::apply_with_backend;
use super::Settle;

fn owned_stage() -> (Fixture, Session) {
    let fx = fixture();
    let record = fx.record(SessionType::Stage, SessionStatus::Running);
    fx.stage(StageStatus::Executing, Some(&record.id));
    (fx, record)
}

fn start_payload() -> WorkerControlPayload {
    WorkerControlPayload::Start {
        unit_id: "unit-a".into(),
        brief_path: "/x/brief.md".into(),
        brief_sha256: "0".repeat(64),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
    }
}

fn cancel_payload(invocation_id: &str) -> WorkerControlPayload {
    WorkerControlPayload::Cancel {
        unit_id: "unit-a".into(),
        invocation_id: invocation_id.into(),
        reason: "stop".into(),
    }
}

fn message_payload(invocation_id: &str, message_id: &str) -> WorkerControlPayload {
    WorkerControlPayload::Message {
        unit_id: "unit-a".into(),
        invocation_id: invocation_id.into(),
        message_id: message_id.into(),
        delivery: Delivery::Steer,
        text: "finish the parser".into(),
    }
}

fn apply(
    fx: &Fixture,
    record: &Session,
    payload: WorkerControlPayload,
    backend: fn(&Path) -> CodexBackend,
) -> Settle {
    apply_with_backend(&fx.host(true), STAGE, record, payload, backend)
}

fn refused(settled: Settle) -> String {
    match settled {
        Settle::Refused(reason) => reason,
        other => panic!("expected refusal, got {other:?}"),
    }
}

fn unexpected_backend(_: &Path) -> CodexBackend {
    panic!("this control must not read the Codex backend")
}

#[test]
fn a_subagent_control_is_refused_before_payload_decode() {
    let (fx, record) = owned_stage();
    let mut entry = entry_for(&record, RequestKind::WorkerControl, serde_json::json!({}));
    entry.agent = AgentRole::Subagent;
    let reason = super::apply::admit(&record, &entry).err().unwrap();
    assert!(reason.contains("a 'subagents:control' request from a subagent is never applied"));
    assert!(!fx.work_dir.join("stage-admission").exists());
}

#[test]
fn start_with_legacy_backend_is_refused_naming_the_key() {
    let (fx, record) = owned_stage();
    let reason = refused(apply(&fx, &record, start_payload(), |_| {
        CodexBackend::Legacy
    }));
    assert_eq!(
        reason,
        "codex.backend is legacy; native Codex workers need codex.backend = native"
    );
}

#[test]
fn a_held_stage_lock_refuses_and_asks_to_repeat() {
    let (fx, record) = owned_stage();
    let invocation_id = seed_native_invocation(&fx, "unit-a");
    let _lock = stage_admission::lock(&fx.work_dir, STAGE).unwrap();
    for payload in [
        start_payload(),
        message_payload(&invocation_id, "message-a"),
        cancel_payload(&invocation_id),
    ] {
        let reason = refused(apply(&fx, &record, payload, unexpected_backend));
        assert_eq!(
            reason,
            format!(
                "another loom command is changing stage '{STAGE}'; repeat the command in a moment"
            )
        );
    }
    let dir = store::invocation_dir(&fx.work_dir, STAGE, &invocation_id);
    assert!(store::read_messages(&dir).unwrap().is_empty());
    assert!(store::read_cancel(&dir).unwrap().is_none());
}

#[test]
fn a_session_that_does_not_own_the_stage_is_refused() {
    let (fx, record) = owned_stage();
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.session = Some("someone-else".into());
        Ok(())
    })
    .unwrap();
    let _lock = stage_admission::lock(&fx.work_dir, STAGE).unwrap();
    let reason = refused(apply(&fx, &record, start_payload(), unexpected_backend));
    assert_eq!(
        reason,
        format!(
            "session '{}' is not the active session of stage '{STAGE}'",
            record.id
        )
    );
}

fn prepared_start(fx: &Fixture) -> WorkerControlPayload {
    update_stage(STAGE, &fx.work_dir, |stage| {
        stage.worktree = Some(STAGE.into());
        stage.implementers = Implementers::new(vec![Implementer::Codex, Implementer::Claude]);
        Ok(())
    })
    .unwrap();
    let worktree = fx.repo_root.join(".worktrees").join(STAGE);
    std::fs::create_dir_all(&worktree).unwrap();
    let brief_path = worktree.join("brief.md");
    std::fs::write(&brief_path, b"Implement the parser.\n").unwrap();
    WorkerControlPayload::Start {
        unit_id: "unit-a".into(),
        brief_path,
        brief_sha256: crate::utils::sha256_hex(b"Implement the parser.\n"),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
    }
}

#[test]
fn start_admits_then_repeats() {
    let (fx, record) = owned_stage();
    let payload = prepared_start(&fx);
    let Settle::Applied(Some(note)) =
        apply(&fx, &record, payload.clone(), |_| CodexBackend::Native)
    else {
        panic!("a valid native start must be admitted");
    };
    let invocation_id = note
        .strip_prefix("native worker unit-a admitted as ")
        .unwrap();
    assert!(invocation_id.starts_with("inv-"));
    let dir = store::invocation_dir(&fx.work_dir, STAGE, invocation_id);
    assert_eq!(
        store::read_manifest(&dir).unwrap().owner_session_id,
        record.id
    );
    assert_eq!(
        apply(&fx, &record, payload, |_| CodexBackend::Native),
        Settle::Applied(Some(format!(
            "native worker unit-a already admitted as {invocation_id}"
        )))
    );
}

#[test]
fn message_and_cancel_notes_match_common_md() {
    let (fx, record) = owned_stage();
    let invocation_id = seed_native_invocation(&fx, "unit-a");
    let message_id = "message-a";
    let message = message_payload(&invocation_id, message_id);
    assert_eq!(
        apply(&fx, &record, message.clone(), unexpected_backend),
        Settle::Applied(Some(format!(
            "message {message_id} accepted for {invocation_id}"
        )))
    );
    assert_eq!(
        apply(&fx, &record, message, unexpected_backend),
        Settle::Applied(Some(format!(
            "message {message_id} accepted for {invocation_id}"
        )))
    );
    assert_eq!(
        apply(
            &fx,
            &record,
            cancel_payload(&invocation_id),
            unexpected_backend
        ),
        Settle::Applied(Some(format!("cancel requested for {invocation_id}")))
    );
    let dir = store::invocation_dir(&fx.work_dir, STAGE, &invocation_id);
    let messages = store::read_messages(&dir).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].message_id, message_id);
    assert_eq!(messages[0].delivery, Delivery::Steer);
    assert_eq!(messages[0].text, "finish the parser");
    let cancel = store::read_cancel(&dir).unwrap().unwrap();
    assert_eq!(cancel.by, format!("lead:{}", record.id));
    assert_eq!(cancel.reason, "stop");
}

#[test]
fn cancel_after_the_backend_became_legacy_still_applies() {
    let (fx, record) = owned_stage();
    let invocation_id = seed_native_invocation(&fx, "unit-a");
    assert_eq!(
        apply(&fx, &record, cancel_payload(&invocation_id), |_| {
            CodexBackend::Legacy
        }),
        Settle::Applied(Some(format!("cancel requested for {invocation_id}")))
    );
}

#[test]
fn decodes_worker_start() {
    let wire = serde_json::json!({
        "op": "start", "unit_id": "unit-a", "brief_path": "/x/brief.md",
        "brief_sha256": "0".repeat(64), "model": "gpt-6.1-terra", "effort": "xhigh",
    });
    assert_eq!(
        decode_payload(RequestKind::WorkerControl, &wire).unwrap(),
        RequestPayload::WorkerControl(start_payload())
    );
}

#[test]
fn decodes_worker_message() {
    let invocation_id = format!("inv-{}", "0".repeat(32));
    let message_id = format!("msg-{}", "1".repeat(32));
    let wire = serde_json::json!({
        "op": "message", "unit_id": "unit-a", "invocation_id": invocation_id,
        "message_id": message_id, "delivery": "next_turn", "text": "continue",
    });
    assert_eq!(
        decode_payload(RequestKind::WorkerControl, &wire).unwrap(),
        RequestPayload::WorkerControl(WorkerControlPayload::Message {
            unit_id: "unit-a".into(),
            invocation_id,
            message_id,
            delivery: Delivery::NextTurn,
            text: "continue".into(),
        })
    );
}

#[test]
fn decodes_worker_cancel() {
    let invocation_id = format!("inv-{}", "0".repeat(32));
    let wire = serde_json::json!({
        "op": "cancel", "unit_id": "unit-a", "invocation_id": invocation_id, "reason": "stop",
    });
    assert_eq!(
        decode_payload(RequestKind::WorkerControl, &wire).unwrap(),
        RequestPayload::WorkerControl(cancel_payload(&invocation_id))
    );
}

#[test]
fn worker_decode_refuses_an_unknown_op_and_unknown_fields() {
    assert!(decode_payload(
        RequestKind::WorkerControl,
        &serde_json::json!({"op": "restart", "unit_id": "unit-a"})
    )
    .is_err());
    let invocation_id = "inv-00000000000000000000000000000000";
    for payload in [
        start_payload(),
        message_payload(invocation_id, "message-a"),
        cancel_payload(invocation_id),
    ] {
        let mut wire = serde_json::to_value(payload).unwrap();
        wire["unexpected"] = serde_json::json!(true);
        assert!(decode_payload(RequestKind::WorkerControl, &wire).is_err());
    }
}
