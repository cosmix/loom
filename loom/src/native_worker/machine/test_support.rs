use super::*;
use crate::native_worker::model::{Delivery, MessageState};
use serde_json::json;

pub(super) fn config() -> MachineConfig {
    MachineConfig {
        cwd: PathBuf::from("/worktree"),
        model: "codex-model".into(),
        effort: "high".into(),
        task_text: "Implement the task".into(),
        client_version: "test-version".into(),
        sandbox: wire::SandboxMode::WorkspaceWrite,
    }
}

pub(super) fn ok(id: u64, value: Value) -> Input {
    Input::Frame(Incoming::Response {
        id,
        result: Ok(value),
    })
}

pub(super) fn err(id: u64, code: i64, message: &str) -> Input {
    Input::Frame(Incoming::Response {
        id,
        result: Err(RpcError {
            code,
            message: message.into(),
        }),
    })
}

pub(super) fn notify(method: &str, params: Value) -> Input {
    Input::Frame(Incoming::Notification {
        method: method.into(),
        params,
    })
}

pub(super) fn turn_started(thread: &str, turn: &str) -> Input {
    notify(
        "turn/started",
        json!({"threadId": thread, "turn": {"id": turn}}),
    )
}

pub(super) fn turn_completed(thread: &str, turn: &str, status: &str) -> Input {
    notify(
        "turn/completed",
        json!({"threadId": thread, "turn": {"id": turn, "status": status}}),
    )
}

pub(super) fn agent_item(
    thread: &str,
    turn: &str,
    id: &str,
    text: &str,
    phase: Option<&str>,
) -> Input {
    notify(
        "item/completed",
        json!({"threadId": thread, "turnId": turn,
        "item": {"type": "agentMessage", "id": id, "text": text, "phase": phase}}),
    )
}

pub(super) fn steer(id: &str, text: &str) -> Input {
    Input::Control(ControlMessage {
        message_id: id.into(),
        delivery: Delivery::Steer,
        text: text.into(),
        sha256: crate::utils::sha256_hex(text.as_bytes()),
        accepted_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    })
}

pub(super) fn next_turn(id: &str, text: &str) -> Input {
    let Input::Control(mut message) = steer(id, text) else {
        unreachable!()
    };
    message.delivery = Delivery::NextTurn;
    Input::Control(message)
}

pub(super) fn started() -> Machine {
    let mut machine = Machine::new(config());
    machine.step(Input::Start);
    machine
}

pub(super) fn thread_ready() -> Machine {
    let mut machine = started();
    machine.step(ok(1, json!({})));
    machine.step(ok(2, json!({"thread": {"id": "th-1"}})));
    machine
}

pub(super) fn active() -> Machine {
    let mut machine = thread_ready();
    machine.step(ok(3, json!({"turn": {"id": "t1"}})));
    machine
}

pub(super) fn methods(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Send(frame) => frame["method"].as_str().map(str::to_owned),
            _ => None,
        })
        .collect()
}

pub(super) fn persisted(actions: &[Action]) -> Vec<NativeEvent> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Persist(event) => Some(event.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn teardown(actions: &[Action]) -> Option<(Outcome, String)> {
    actions.iter().find_map(|action| match action {
        Action::Teardown { outcome, reason } => Some((*outcome, reason.clone())),
        _ => None,
    })
}

pub(super) fn sent<'a>(actions: &'a [Action], method: &str) -> &'a Value {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Send(frame) if frame["method"] == method => Some(frame),
            _ => None,
        })
        .unwrap()
}

pub(super) fn message_event(
    id: &str,
    state: MessageState,
    turn: Option<&str>,
    detail: Option<&str>,
) -> NativeEvent {
    NativeEvent::Message {
        message_id: id.into(),
        state,
        turn_id: turn.map(str::to_owned),
        detail: detail.map(str::to_owned),
    }
}
