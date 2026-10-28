//! Contracts of stage bridge-runtime: the bridge's queue, interrupt, permission, ending and
//! recovery rules over a scripted host, the launch script and the planning-socket skip
//! (PLAN-feat-planner-07-session-bridge).

#[path = "plan_session_bridge_runtime_contracts/host.rs"]
mod host;
#[path = "plan_session_bridge_runtime_contracts/launch.rs"]
mod launch;
#[path = "plan_session_bridge_runtime_contracts/support.rs"]
mod support;
#[path = "plan_session_bridge_runtime_contracts/util.rs"]
mod util;

use std::fs;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use loom::orchestrator::terminal::tmux::list_loom_sockets;
use loom::plan_session::bridge::{BridgeError, EndCause, EndMode, SendOutcome};
use loom::plan_session::kind::SessionKind::Authoring;
use loom::plan_session::model::{DenyReason, SessionState};
use loom::plan_session::permission::Behavior;
use loom::process::{process_start_time, IdentityStatus, ProcessIdentity};
use serde_json::json;
use serial_test::serial;
use support::*;
use EndMode::{FinalWrite, Teardown};
use SessionState::{Failed, Permission, Working};

#[test]
fn a_message_sent_while_working_is_delivered_at_the_next_idle() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    assert!(matches!(h.send(&s, "A"), SendOutcome::Delivering { .. }));
    wait_until("A delivered", || h.sent(&s) == ["A"]);
    h.working(&s);
    assert!(matches!(h.send(&s, "B"), SendOutcome::Queued { .. }));
    settle();
    assert_eq!(h.sent(&s), ["A"], "typed into the pane while working");
    h.spool(&s, "Stop", json!({}));
    wait_until("B delivered after the Stop", || h.sent(&s).len() == 2);
    assert_eq!(h.sent(&s), ["A", "B"]);
}

#[test]
fn interrupt_returns_queued_texts_and_delivers_nothing_at_the_next_idle() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    for text in ["X", "Y"] {
        assert!(matches!(h.send(&s, text), SendOutcome::Queued { .. }));
    }
    let request = h.permission(&s, "Bash", json!({"command": "rm -rf build"}));
    h.wait_state(&s, Permission);
    let turn = h.pins.begins().pop().expect("the open turn was pinned");
    assert_eq!(h.interrupt(&s).unwrap(), ["X", "Y"]);
    h.wait_deny(&s, &request, DenyReason::Interrupt);
    wait_until("the Escape press", || h.host.interrupts(&s.socket) == 1);
    let ended = h.turns_ended(&s);
    h.record(&s, interrupted());
    wait_until("TurnEnded with no Stop", || h.turns_ended(&s) > ended);
    settle();
    assert_eq!(h.turns_ended(&s), ended + 1);
    assert_eq!(h.pins.ends().iter().filter(|t| **t == turn).count(), 1);
    assert_eq!(h.host.interrupts(&s.socket), 1);
    assert_eq!(h.sent(&s), Vec::<String>::new(), "delivered at the Idle");
}

#[test]
fn interrupt_is_refused_while_idle_and_presses_once_per_turn() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    assert!(matches!(h.send(&s, "A"), SendOutcome::Delivering { .. }));
    wait_until("A delivered", || h.sent(&s) == ["A"]);
    assert!(matches!(h.send(&s, "B"), SendOutcome::Queued { .. }));
    let calls = h.host.calls().len();
    assert!(matches!(h.interrupt(&s), Err(BridgeError::NotRunning)));
    settle();
    assert_eq!(
        h.host.calls().len(),
        calls,
        "an Idle interrupt reached the host"
    );
    h.working(&s);
    assert_eq!(h.interrupt(&s).unwrap(), ["B"], "the queue was lost");
    let _ = h.interrupt(&s);
    settle();
    assert_eq!(h.host.interrupts(&s.socket), 1, "two Escapes in one turn");
}

#[test]
fn end_returns_the_queue_and_delivers_only_the_closing_message() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    assert!(matches!(h.send(&s, "B"), SendOutcome::Queued { .. }));
    assert_eq!(h.end(&s, FinalWrite, EndCause::Holder), ["B"]);
    let late = h.bridge.send(&s.id, "C".to_owned(), Vec::new());
    assert!(matches!(late, Err(BridgeError::NotRunning)));
    settle();
    assert_eq!(h.sent(&s), Vec::<String>::new(), "delivered mid-turn");
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || !h.sent(&s).is_empty());
    settle();
    assert_eq!(h.sent(&s), [CLOSING]);
}

#[test]
fn another_tools_result_does_not_close_a_pending_request() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let bash = json!({"command": "cargo test --lib"});
    let read = json!({"file_path": "doc/a.md"});
    let tools = [("toolu_a", "Read", read), ("toolu_b", "Bash", bash.clone())];
    h.record(&s, assistant_tools(&tools));
    settle();
    let b = h.permission(&s, "Bash", bash);
    h.wait_state(&s, Permission);
    thread::sleep(Duration::from_secs(3));
    h.record(&s, tool_result("toolu_a"));
    settle();
    assert_eq!(h.state(&s), Some(Permission), "A's result closed B");
    assert!(h.answer(&s, &b).is_none(), "B was answered by A's result");
    h.bridge.answer_permission(&s.id, &b, true, None).unwrap();
    let answer = wait_for("B's allow", || h.answer(&s, &b));
    assert_eq!(answer.behavior, Behavior::Allow);
    h.wait_state(&s, Working);
    let c = h.permission(&s, "Bash", json!({"command": "cargo build"}));
    h.wait_state(&s, Permission);
    h.spool(&s, "Stop", json!({}));
    h.wait_deny(&s, &c, DenyReason::SessionEnded);
}

#[test]
fn grace_end_denies_pending_and_new_requests_so_the_closing_write_runs() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let first = h.permission(&s, "Bash", json!({"command": "make one"}));
    h.wait_state(&s, Permission);
    h.end(&s, FinalWrite, EndCause::GraceExpired);
    h.wait_deny(&s, &first, DenyReason::Holder);
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    let second = h.permission(&s, "Bash", json!({"command": "make two"}));
    h.wait_deny(&s, &second, DenyReason::Holder);
}

#[test]
fn end_delivers_the_closing_message_only_after_idle_and_tears_down_after_stop() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let agent = json!({"agent_id": "a1", "agent_type": "general-purpose"});
    h.spool(&s, "SubagentStart", agent.clone());
    settle();
    h.end(&s, FinalWrite, EndCause::Holder);
    settle();
    assert_eq!(h.sent(&s), Vec::<String>::new(), "pasted mid-turn");
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    h.spool(&s, "SubagentStop", agent);
    settle();
    assert_eq!(h.host.teardowns(&s.socket), 0, "torn down before the Stop");
    h.spool(&s, "Stop", json!({}));
    wait_until("the teardown", || h.host.teardowns(&s.socket) == 1);
}

#[test]
fn end_tears_down_at_the_bound_without_a_stop() {
    let h = Fixture::new().ready(|c| c.final_write_bound = Duration::from_secs(1));
    let s = h.idle(Authoring);
    let asked = Instant::now();
    h.end(&s, FinalWrite, EndCause::Holder);
    wait_until("the teardown", || h.host.teardowns(&s.socket) == 1);
    let waited = asked.elapsed();
    assert!(
        waited < Duration::from_secs(3),
        "torn down after {waited:?}"
    );
    wait_for("the Ended event", || h.ended(&s.id));
}

#[test]
fn a_pressure_session_ends_when_its_marker_appears_without_a_closing_message() {
    let h = Fixture::new().ready(|_| {});
    let first = h.idle(pressure());
    touch_marker(&first);
    assert_eq!(wait_for("the marker end", || h.ended(&first.id)), "marker");
    h.bridge.release_session(&first.id).unwrap();
    let second = h.idle(pressure());
    h.end(&second, FinalWrite, EndCause::Holder);
    wait_until("the teardown", || h.host.teardowns(&second.socket) == 1);
    settle();
    for s in [&first, &second] {
        assert_eq!(h.sent(s), Vec::<String>::new(), "a pressure delivery");
    }
}

#[test]
fn a_process_gone_outside_ending_fails_the_session_and_denies_pending_requests() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    let request = h.permission(&s, "Bash", json!({"command": "ls"}));
    h.wait_state(&s, Permission);
    h.host.set_identity(s.pid, IdentityStatus::Dead);
    h.wait_state(&s, Failed);
    assert_eq!(wait_for("the Ended event", || h.ended(&s.id)), "failed");
    h.wait_deny(&s, &request, DenyReason::SessionEnded);
    settle();
    assert_eq!(h.host.spawns(), 1, "the bridge restarted the session");
    assert_eq!(h.state(&s), Some(Failed));
}

#[test]
fn recovery_leaves_an_unverifiable_record_and_its_directory_alone() {
    let f = Fixture::new();
    let unsure = f.write_record(Authoring, Some(3_800_001));
    let dead = f.write_record(Authoring, Some(3_800_002));
    f.host
        .set_identity(unsure.pid, IdentityStatus::Unverifiable);
    f.host.set_identity(dead.pid, IdentityStatus::Dead);
    f.host.set_socket(&unsure.socket, true);
    f.host.set_socket(&dead.socket, true);
    let h = f.start(|_| {});
    let report = h.bridge.recover();
    let left = &report.left_alone;
    let named =
        |(id, socket, _): &(String, String, String)| *id == unsure.id && *socket == unsure.socket;
    assert!(left.iter().any(named), "left_alone: {left:?}");
    assert!(
        unsure.dir.record().is_file(),
        "the unverifiable record went"
    );
    assert_eq!(
        h.host.teardowns(&unsure.socket),
        0,
        "unverifiable torn down"
    );
    assert!(report.torn_down.iter().any(|r| r.id == dead.id));
    assert!(
        !dead.dir.path().exists(),
        "the dead session's directory stayed"
    );
}

#[test]
fn a_failed_teardown_keeps_the_session_record_and_directory() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(Authoring);
    h.host.fail_teardown();
    h.end(&s, Teardown, EndCause::Caller);
    wait_until("the teardown attempt", || h.host.teardowns(&s.socket) >= 1);
    h.wait_state(&s, Failed);
    settle();
    assert!(
        s.dir.record().is_file(),
        "session.json went with a failed teardown"
    );
    assert!(s.dir.path().is_dir());
    assert_eq!(h.state(&s), Some(Failed), "left Ending or Ended");
    h.start(Authoring);
}

#[test]
fn a_marker_end_keeps_the_recorded_child_and_its_log_until_release() {
    let h = Fixture::new().ready(|_| {});
    let s = h.idle(pressure());
    let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
    let pid = child.0.id();
    let start_time = process_start_time(pid);
    assert!(start_time.is_some(), "no start time for the child");
    let identity = ProcessIdentity { pid, start_time };
    h.bridge.record_child(&s.id, "codex", identity).unwrap();
    fs::write(s.dir.codex_log(), "codex output\n").unwrap();
    touch_marker(&s);
    assert_eq!(wait_for("the marker end", || h.ended(&s.id)), "marker");
    settle();
    assert!(child.alive(), "the marker end terminated the Codex child");
    let log = fs::read_to_string(s.dir.codex_log()).unwrap();
    assert_eq!(log, "codex output\n");
    h.bridge.release_session(&s.id).unwrap();
    wait_until("the child terminated", || !child.alive());
    wait_until("the directory removed", || !s.dir.path().exists());
}

#[test]
fn recovery_tears_down_a_provisional_record_whose_socket_exists() {
    let f = Fixture::new();
    let provisional = f.write_record(Authoring, None);
    f.host.set_socket(&provisional.socket, true);
    let bare = f.bare_dir();
    let h = f.start(|_| {});
    let report = h.bridge.recover();
    let teardowns = h.host.teardowns(&provisional.socket);
    assert_eq!(teardowns, 1, "the live socket was left behind");
    wait_for("the Ended event", || h.ended(&provisional.id));
    let left = &report.left_alone;
    assert!(
        left.iter().any(|(id, ..)| *id == bare),
        "left_alone: {left:?}"
    );
}

#[test]
fn recovery_denies_an_unanswered_request_before_the_final_write() {
    let f = Fixture::new();
    let s = f.write_record(Authoring, Some(3_800_010));
    f.spool(&s, "SessionStart", json!({"source": "startup"}));
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "go on"}));
    let request = f.permission(&s, "Bash", json!({"command": "make all"}));
    let h = f.start(|c| c.final_write_bound = Duration::from_secs(10));
    let (tx, rx) = mpsc::channel();
    let bridge = h.bridge.clone();
    thread::spawn(move || tx.send(bridge.recover().closed.len()));
    h.wait_deny(&s, &request, DenyReason::ServerStopping);
    assert_eq!(h.sent(&s), Vec::<String>::new(), "closing before the deny");
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    h.spool(&s, "Stop", json!({}));
    let finished = rx.recv_timeout(Duration::from_secs(15));
    finished.expect("recovery never finished");
}

#[test]
fn launch_script_escapes_the_opening_message_and_drops_stage_variables() {
    let name = "-it's $(touch pwned)\nsecond line";
    let opening = "OPENING-7f3a: read doc/plans/PLAN-x.md";
    let run = launch::run_launch_script(name, opening, &launch::parent_env());
    let at = run.argv.iter().position(|a| a == "--name");
    let at = at.expect("no --name argument");
    assert_eq!(run.argv[at + 1], name, "argv: {:?}", run.argv);
    for dir in [&run.root, &run.tmp] {
        assert!(!dir.join("pwned").exists(), "the name ran a substitution");
    }
    assert!(
        !run.script.contains("OPENING-7f3a"),
        "the opening is in launch.sh"
    );
    let on_argv = run.argv.iter().any(|a| a.contains("OPENING-7f3a"));
    assert!(!on_argv, "the opening is on argv");
    let state = run.state.display();
    let expected = format!("session={}\nstate={state}\nstage=\n", run.id);
    assert_eq!(run.env, expected, "LOOM_PLAN_* missing or stage inherited");
}

#[test]
#[serial]
fn list_loom_sockets_skips_planning_sockets() {
    let dir = host::socket_dir();
    for name in ["loom-plan-3f9a0c41b7e2", "loom-session-abc"] {
        fs::write(dir.join(name), b"").unwrap();
    }
    let work = tempfile::tempdir().unwrap();
    let sockets = list_loom_sockets(work.path());
    let listed: Vec<String> = sockets.into_iter().map(|s| s.session_id).collect();
    assert_eq!(listed, ["session-abc"]);
}
