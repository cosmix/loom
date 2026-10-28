//! Bridge tests: recovery of a crashed server's sessions and its report.

use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::json;

use super::fake_host::*;
use super::registry::{read_record, write_record, ChildRecord, SessionRecord, SCHEMA};
use super::{BridgeError, BridgeEvent, EndCause, EndMode, EndOutcome, RecoveryReport};
use crate::plan_session::kind::SessionKind::{Authoring, Summary};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::DenyReason;
use crate::plan_session::permission::{write_answer, Answer, Behavior};
use crate::process::{process_start_time, IdentityStatus};

/// A child the test owns; killed and reaped on drop.
struct OwnedChild(Child);

impl OwnedChild {
    fn alive(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `session.json` and `answers/` as a crashed server left them.
fn recorded(h: &Harness, kind: SessionKind, pid: Option<u32>) -> Sess {
    let id = uuid::Uuid::new_v4().to_string();
    let dir = SessionDir::new(h.runtime.sessions.join(&id));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir.answers())
        .unwrap();
    let socket = format!("loom-plan-{}", &id.replace('-', "")[..12]);
    let transcript = h.transcript_of(&id);
    fs::write(&transcript, b"").unwrap();
    let record = SessionRecord {
        schema: SCHEMA,
        id: id.clone(),
        tmux_session: format!("plan-{}", kind.label()),
        kind,
        socket: socket.clone(),
        pid,
        start_time: pid.map(|_| 42),
        transcript_path: Some(transcript.clone()),
        started_at: chrono::Utc::now().to_rfc3339(),
        children: Vec::new(),
    };
    write_record(&dir, &record).unwrap();
    let pid = pid.unwrap_or(0);
    Sess {
        id,
        dir,
        socket,
        pid,
        transcript,
    }
}

fn torn_down(h: &Harness, id: &str) -> bool {
    h.ended(id) == Some(EndOutcome::TornDown)
}

#[test]
fn unverifiable_is_left_alone_dead_is_cleaned_and_alive_is_closed() {
    let h = Harness::new(|c| c.final_write_bound = Duration::from_secs(1));
    let unsure = recorded(&h, Authoring, Some(3_800_101));
    h.host
        .set_identity(unsure.pid, IdentityStatus::Unverifiable);
    let dead = recorded(&h, Summary, Some(3_800_102));
    h.host.set_identity(dead.pid, IdentityStatus::Dead);
    let alive = recorded(&h, Authoring, Some(3_800_103));
    h.spool(&alive, "SessionStart", json!({"source": "startup"}));
    let report = h.bridge.recover();
    let left = &report.left_alone;
    let why = "process identity cannot be verified".to_owned();
    assert_eq!(*left, [(unsure.id.clone(), unsure.socket.clone(), why)]);
    assert!(unsure.dir.record().is_file());
    assert_eq!(h.host.count(&Call::Teardown(unsure.socket.clone())), 0);
    assert_eq!(report.torn_down.len(), 1);
    assert_eq!(report.torn_down[0].id, dead.id);
    assert!(!dead.dir.path().exists());
    assert!(torn_down(&h, &dead.id), "no Ended for the dead session");
    assert_eq!(report.closed.len(), 1);
    assert_eq!(report.closed[0].id, alive.id);
    assert_eq!(h.sent(&alive), [CLOSING]);
    assert!(
        h.ended(&alive.id).is_some(),
        "no Ended for the closed session"
    );
    assert!(!alive.dir.path().exists());
}

#[test]
fn a_start_waits_for_recovery() {
    let h = Harness::new(|c| c.final_write_bound = Duration::from_secs(1));
    let alive = recorded(&h, Authoring, Some(3_800_201));
    let bridge = h.bridge.clone();
    let recovering = thread::spawn(move || bridge.recover());
    let listed = || h.bridge.sessions().iter().any(|(id, ..)| *id == alive.id);
    wait_until("the recovered session", listed);
    let refused = h.bridge.start_session(h.request(Authoring, OPENING, false));
    assert!(matches!(refused, Err(BridgeError::Busy(_))), "{refused:?}");
    let report = recovering.join().unwrap();
    assert_eq!(report.closed.len(), 1);
    h.start(Authoring);
}

#[test]
fn a_live_pressure_session_is_torn_down_with_its_child() {
    let h = Harness::new(|_| {});
    let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
    let s = recorded(&h, pressure(), Some(3_800_301));
    let mut record = read_record(&s.dir).unwrap();
    let pid = child.0.id();
    let label = "codex".to_owned();
    let start_time = process_start_time(pid);
    record.children.push(ChildRecord {
        label,
        pid,
        start_time,
    });
    write_record(&s.dir, &record).unwrap();
    let report = h.bridge.recover();
    assert_eq!(report.torn_down.len(), 1);
    assert_eq!(report.torn_down[0].children.len(), 1);
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
    assert!(
        h.host.delivered(&s.socket).is_empty(),
        "a pressure session got a closing message"
    );
    wait_until("the child terminated", || !child.alive());
    assert!(!s.dir.path().exists());
    wait_until("the Ended event", || torn_down(&h, &s.id));
}

#[test]
fn a_directory_without_a_record_or_with_a_foreign_socket_is_left_alone() {
    let h = Harness::new(|_| {});
    let bare = uuid::Uuid::new_v4().to_string();
    let bare_dir = h.runtime.sessions.join(&bare);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&bare_dir)
        .unwrap();
    let foreign = recorded(&h, Authoring, None);
    let mut record = read_record(&foreign.dir).unwrap();
    record.socket = "loom-session-abc".to_owned();
    write_record(&foreign.dir, &record).unwrap();
    let report = h.bridge.recover();
    let ids: Vec<&str> = report
        .left_alone
        .iter()
        .map(|(id, ..)| id.as_str())
        .collect();
    assert!(
        ids.contains(&bare.as_str()) && ids.contains(&foreign.id.as_str()),
        "{ids:?}"
    );
    assert_eq!(
        h.host.count(&Call::Teardown("loom-session-abc".to_owned())),
        0
    );
    assert!(bare_dir.is_dir() && foreign.dir.record().is_file());
}

#[test]
fn a_provisional_record_without_a_socket_is_cleaned() {
    let h = Harness::new(|_| {});
    let provisional = recorded(&h, Authoring, None);
    let report = h.bridge.recover();
    assert_eq!(report.torn_down.len(), 1);
    assert_eq!(h.host.count(&Call::Teardown(provisional.socket.clone())), 0);
    assert!(!provisional.dir.path().exists());
}

#[test]
fn a_recovered_session_denies_unanswered_requests_and_keeps_answered_ones() {
    let h = Harness::new(|c| c.final_write_bound = Duration::from_secs(10));
    let s = recorded(&h, Authoring, Some(3_800_401));
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    h.spool(&s, "UserPromptSubmit", json!({"prompt": "go on"}));
    let answered = h.permission(&s, "Bash", json!({"command": "one"}));
    let allow = Answer {
        behavior: Behavior::Allow,
        message: None,
        reason: None,
    };
    write_answer(s.dir.path(), &answered, &allow).unwrap();
    let open = h.permission(&s, "Bash", json!({"command": "two"}));
    let bridge = h.bridge.clone();
    let recovering = thread::spawn(move || bridge.recover());
    h.wait_deny(&s, &open, DenyReason::ServerStopping);
    assert_eq!(h.answer(&s, &answered).unwrap().behavior, Behavior::Allow);
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    let arriving = h.permission(&s, "Bash", json!({"command": "three"}));
    h.wait_deny(&s, &arriving, DenyReason::ServerStopping);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    h.spool(&s, "Stop", json!({}));
    let report = recovering.join().unwrap();
    assert_eq!(report.closed.len(), 1);
    assert_eq!(h.ended(&s.id), Some(EndOutcome::Closed));
}

#[test]
fn on_recovered_runs_after_recovery_or_at_once() {
    let h = Harness::new(|_| {});
    let reports: Arc<Mutex<Vec<RecoveryReport>>> = Arc::default();
    let early = reports.clone();
    h.bridge
        .on_recovered(Box::new(move |report: &RecoveryReport| {
            early.lock().unwrap().push(report.clone());
        }));
    assert!(reports.lock().unwrap().is_empty());
    h.bridge.recover();
    assert_eq!(reports.lock().unwrap().len(), 1);
    let late = reports.clone();
    h.bridge
        .on_recovered(Box::new(move |report: &RecoveryReport| {
            late.lock().unwrap().push(report.clone());
        }));
    assert_eq!(reports.lock().unwrap().len(), 2);
    let again = h.bridge.recover();
    assert_eq!(again, RecoveryReport::default());
    let ended = h.count_events(|e| matches!(e, BridgeEvent::Ended { .. }));
    assert_eq!(ended, 0);
}

#[test]
fn a_panicking_listener_stops_neither_recovery_nor_the_driver() {
    let h = Harness::new(|_| {});
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    h.bridge.subscribe(Box::new(move |_: &BridgeEvent| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("a listener's first call panics");
        }
    }));
    let heard: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
    let second = heard.clone();
    h.bridge.subscribe(Box::new(move |event: &BridgeEvent| {
        second.lock().unwrap().push(event.clone());
    }));
    let ended = |id: &str| {
        let heard = heard.lock().unwrap();
        heard.iter().any(|event| match event {
            BridgeEvent::Ended { id: ended, .. } => ended == id,
            _ => false,
        })
    };
    // Recovery cleans a dead server's provisional record; its flush meets the panic first.
    let provisional = recorded(&h, Authoring, None);
    h.bridge.recover();
    assert!(
        ended(&provisional.id),
        "the second listener missed the Ended"
    );
    let s = h.idle(Authoring);
    h.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the end", || ended(&s.id));
    assert!(calls.load(Ordering::SeqCst) > 1, "the listener was dropped");
}
