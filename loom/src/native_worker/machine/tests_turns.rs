use super::test_support::*;
use super::*;
use serde_json::json;

#[test]
fn completion_before_turn_started_learns_and_completes_once() {
    let mut machine = thread_ready();
    let actions = machine.step(turn_completed("th-1", "t1", "completed"));
    assert_eq!(
        persisted(&actions),
        [
            NativeEvent::TurnStarted {
                turn_id: "t1".into()
            },
            NativeEvent::TurnCompleted {
                turn_id: "t1".into(),
                status: "completed".into()
            }
        ]
    );
    assert_eq!(
        teardown(&actions),
        Some((Outcome::Succeeded, "turn completed".into()))
    );
    assert!(machine
        .step(ok(3, json!({"turn": {"id": "t1"}})))
        .is_empty());
}

#[test]
fn turn_response_without_id_tears_down_failed() {
    assert_eq!(
        teardown(&thread_ready().step(ok(3, json!({})))),
        Some((
            Outcome::Failed,
            "turn/start response missing turn.id".into()
        ))
    );
}

/// Both protocol messages carry the configured, launch-confirmed mode: a
/// confirmed Seatbelt launch asks for no nested Codex sandbox on either.
#[test]
fn a_confirmed_seatbelt_launch_sends_danger_full_access_on_thread_and_turn() {
    let mut machine = Machine::new(MachineConfig {
        sandbox: wire::SandboxMode::DangerFullAccess,
        ..config()
    });
    machine.step(Input::Start);
    let init = machine.step(ok(1, json!({})));
    assert_eq!(
        sent(&init, "thread/start")["params"]["sandbox"],
        "danger-full-access"
    );
    let thread = machine.step(ok(2, json!({"thread": {"id": "th-1"}})));
    let policy = &sent(&thread, "turn/start")["params"]["sandboxPolicy"];
    assert_eq!(policy, &json!({"type": "dangerFullAccess"}));
}

/// The Linux mode keeps Codex's own workspace sandbox on both messages.
#[test]
fn the_linux_mode_sends_workspace_write_on_thread_and_turn() {
    let init = started().step(ok(1, json!({})));
    assert_eq!(
        sent(&init, "thread/start")["params"]["sandbox"],
        "workspace-write"
    );
    let thread = thread_ready_actions();
    let policy = &sent(&thread, "turn/start")["params"]["sandboxPolicy"];
    assert_eq!(policy["type"], "workspaceWrite");
    assert_eq!(policy["networkAccess"], false);
}

fn thread_ready_actions() -> Vec<Action> {
    let mut machine = started();
    machine.step(ok(1, json!({})));
    machine.step(ok(2, json!({"thread": {"id": "th-1"}})))
}

#[test]
fn malformed_notification_and_completion_for_other_turn_are_ignored() {
    let mut machine = active();
    assert!(machine.step(notify("turn/completed", json!({}))).is_empty());
    assert!(machine
        .step(turn_completed("th-1", "other", "completed"))
        .is_empty());
    assert!(!machine.is_finished());
}
