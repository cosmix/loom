//! Contract tests for the native worker core: ids, record store, index, framing, policy and
//! the reducer. Frozen before the implementation; see common.md sections 5 to 11.

use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::ids::validate_unit_id;
use super::index::{stage_disposition, NativeDisposition};
use super::machine::{Action, Input, Machine, MachineConfig};
use super::model::{ControlMessage, Delivery, MessageState, NativeEvent, NativeManifest, Outcome};
use super::protocol::frame::{read_frame, FrameError, Incoming};
use super::protocol::policy::{reply_for, ServerReply};
use super::store::{create_invocation, EventLog};

const STAGE: &str = "native-core";
const INVOCATION: &str = "inv-0123456789abcdef0123456789abcdef";
const THREAD: &str = "thr-1";
const TURN: &str = "turn-1";

fn manifest(workspace: &Path) -> NativeManifest {
    let brief = b"brief";
    NativeManifest {
        v: 1,
        stage_id: STAGE.to_string(),
        unit_id: "c1".to_string(),
        invocation_id: INVOCATION.to_string(),
        owner_session_id: "session-1".to_string(),
        workspace_root: workspace.to_path_buf(),
        brief_path: workspace.join("brief.md"),
        brief_sha256: hex::encode(Sha256::digest(brief)),
        model: "gpt-6.1-terra".to_string(),
        effort: "xhigh".to_string(),
        deadline_secs: 1500,
        created_at: Utc::now(),
    }
}

/// Returns (temp dir guard, work dir, invocation dir).
fn invocation() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let work = tmp.path().join("work");
    fs::create_dir_all(&work).expect("work dir");
    let dir = create_invocation(&work, &manifest(tmp.path()), b"brief").expect("create");
    (tmp, work, dir)
}

#[test]
fn unit_id_rejects_traversal() {
    let long = "a".repeat(65);
    for bad in ["", "..", "a/b", "-lead", long.as_str()] {
        assert!(validate_unit_id(bad).is_err(), "accepted {bad:?}");
    }
    assert_eq!(validate_unit_id("T0a_1-x"), Ok(()));
    assert_eq!(validate_unit_id(&"a".repeat(64)), Ok(()));
}

#[test]
fn store_refuses_symlinked_event_log() {
    let (tmp, _work, dir) = invocation();
    let target = tmp.path().join("target.txt");
    fs::write(&target, b"original\n").expect("target");
    let events = dir.join("events.jsonl");
    fs::remove_file(&events).expect("remove events");
    std::os::unix::fs::symlink(&target, &events).expect("symlink");

    let refused = match EventLog::open(&dir) {
        Err(_) => true,
        Ok(mut log) => log.append(NativeEvent::Progress).is_err(),
    };
    assert!(refused, "event log was written through a symlink");
    assert_eq!(fs::read(&target).expect("read target"), b"original\n");
}

#[test]
fn corrupt_record_is_unverified() {
    let (_tmp, work, dir) = invocation();
    let mut log = EventLog::open(&dir).expect("open");
    log.append(NativeEvent::Terminal {
        outcome: Outcome::Succeeded,
        reason: "turn completed".to_string(),
    })
    .expect("terminal");
    log.append(NativeEvent::OwnershipReleased {
        by: "supervisor".to_string(),
    })
    .expect("released");
    drop(log);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(dir.join("events.jsonl"))
        .expect("open raw");
    file.write_all(b"not json\n").expect("raw append");
    drop(file);

    match stage_disposition(&work, STAGE) {
        NativeDisposition::Unverified(cause) => {
            assert!(
                cause.starts_with(&format!("{INVOCATION}:")),
                "cause {cause:?}"
            );
        }
        other => panic!("expected Unverified, got {other:?}"),
    }
}

#[test]
fn frame_oversized_line_does_not_desync() {
    let filler = "a".repeat(3000 - r#"{"method":""}"#.len());
    let long = format!(r#"{{"method":"{filler}"}}"#);
    assert_eq!(long.len(), 3000);
    let input = format!("{long}\n{}\n", r#"{"method":"initialized"}"#);
    let mut reader = Cursor::new(input.into_bytes());

    match read_frame(&mut reader, 1024) {
        Err(FrameError::Oversized) => {}
        other => panic!("expected Oversized, got {other:?}"),
    }
    let next = read_frame(&mut reader, 1024).expect("next frame");
    assert_eq!(next, json!({"method": "initialized"}));
}

fn expect_result(method: &str, expected: Value) {
    match reply_for(method) {
        ServerReply::Result(value) => assert_eq!(value, expected, "reply to {method}"),
        ServerReply::Error { code, .. } => panic!("{method} got error {code}"),
    }
}

fn expect_error(method: &str, expected: i64) {
    match reply_for(method) {
        ServerReply::Error { code, .. } => assert_eq!(code, expected, "reply to {method}"),
        ServerReply::Result(value) => panic!("{method} got result {value}"),
    }
}

#[test]
fn approval_requests_are_declined_per_method() {
    expect_result(
        "item/commandExecution/requestApproval",
        json!({"decision": "decline"}),
    );
    expect_result("execCommandApproval", json!({"decision": "denied"}));
    expect_error("item/tool/requestUserInput", -32000);
    expect_error("vendor/somethingNew", -32601);
}

fn machine() -> Machine {
    Machine::new(MachineConfig {
        cwd: PathBuf::from("/work/tree"),
        model: "gpt-6.1-terra".to_string(),
        effort: "xhigh".to_string(),
        task_text: "do the task".to_string(),
        client_version: "0.0.0".to_string(),
    })
}

fn sent_id(actions: &[Action], method: &str) -> Option<u64> {
    actions.iter().rev().find_map(|action| match action {
        Action::Send(frame) if frame["method"] == method => frame["id"].as_u64(),
        _ => None,
    })
}

fn ok(id: u64, result: Value) -> Input {
    Input::Frame(Incoming::Response {
        id,
        result: Ok(result),
    })
}

fn note(method: &str, params: Value) -> Input {
    Input::Frame(Incoming::Notification {
        method: method.to_string(),
        params,
    })
}

fn turn_completed(turn: &str) -> Input {
    note(
        "turn/completed",
        json!({"threadId": THREAD, "turn": {"id": turn, "status": "completed", "error": null}}),
    )
}

fn agent_item(id: &str, text: &str, phase: &str) -> Input {
    note(
        "item/completed",
        json!({"threadId": THREAD, "turnId": TURN,
               "item": {"type": "agentMessage", "id": id, "text": text, "phase": phase}}),
    )
}

fn control(message_id: &str, delivery: Delivery) -> Input {
    let text = format!("message {message_id}");
    Input::Control(ControlMessage {
        message_id: message_id.to_string(),
        delivery,
        sha256: hex::encode(Sha256::digest(text.as_bytes())),
        text,
        accepted_at: Utc::now(),
    })
}

/// Start plus the initialize response; returns the thread/start request id.
fn through_initialize(m: &mut Machine) -> u64 {
    let started = m.step(Input::Start);
    let init = sent_id(&started, "initialize").expect("initialize sent");
    let after = m.step(ok(init, json!({"userAgent": "codex"})));
    sent_id(&after, "thread/start").expect("thread/start sent")
}

/// Drives a machine until turn TURN is active.
fn active_turn(m: &mut Machine) {
    let thread = through_initialize(m);
    let after = m.step(ok(thread, json!({"thread": {"id": THREAD}})));
    let turn = sent_id(&after, "turn/start").expect("turn/start sent");
    m.step(ok(turn, json!({"turn": {"id": TURN}})));
}

fn assert_cancelled_last(actions: &[Action]) {
    match actions.last() {
        Some(Action::Teardown {
            outcome: Outcome::Cancelled,
            ..
        }) => {}
        other => panic!("expected Teardown Cancelled last, got {other:?}"),
    }
}

#[test]
fn cancel_before_turn_never_starts_a_turn() {
    let mut m = machine();
    let thread = through_initialize(&mut m);
    let mut actions = m.step(Input::Cancel {
        reason: "operator".to_string(),
    });
    actions.extend(m.step(ok(thread, json!({"thread": {"id": THREAD}}))));

    assert_eq!(sent_id(&actions, "turn/start"), None, "{actions:?}");
    assert_cancelled_last(&actions);
    assert!(m.is_finished());
}

#[test]
fn accepted_cancel_beats_late_success() {
    let mut m = machine();
    active_turn(&mut m);
    m.step(agent_item("i1", "done", "final_answer"));
    let mut actions = m.step(Input::Cancel {
        reason: "operator".to_string(),
    });
    actions.extend(m.step(turn_completed(TURN)));

    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::StoreReport { .. })),
        "{actions:?}"
    );
    assert_cancelled_last(&actions);
}

#[test]
fn steer_after_turn_end_is_rejected() {
    let mut m = machine();
    active_turn(&mut m);
    m.step(control("m1", Delivery::NextTurn));
    let ended = m.step(turn_completed(TURN));
    assert!(sent_id(&ended, "turn/start").is_some(), "{ended:?}");

    let actions = m.step(control("m2", Delivery::Steer));
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Send(_))),
        "{actions:?}"
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Persist(NativeEvent::Message { message_id, state: MessageState::Rejected, .. })
                if message_id == "m2"
        )),
        "{actions:?}"
    );
}

#[test]
fn report_keeps_only_final_answer() {
    let mut m = machine();
    active_turn(&mut m);
    m.step(agent_item("i1", "A", "commentary"));
    m.step(agent_item("i2", "B", "final_answer"));
    m.step(agent_item("i2", "B2", "final_answer"));
    let actions = m.step(turn_completed(TURN));

    let report = actions.iter().find_map(|a| match a {
        Action::StoreReport { text, complete } => Some((text.clone(), *complete)),
        _ => None,
    });
    assert_eq!(report, Some(("B2".to_string(), true)), "{actions:?}");
}
