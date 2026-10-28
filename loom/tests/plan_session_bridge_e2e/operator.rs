//! The operator's run with a logged-in Claude Code: the opening by paste, a queued second
//! message, a permission round, an interrupt, `/compact`, the session's temporary directory,
//! the final write, and whether a pasted plugin slash command runs. Prints what the installed
//! Claude Code does where common.md assumes it.

use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

use loom::plan_session::bridge::registry::read_record;
use loom::plan_session::bridge::{BridgeEvent, EndCause, EndMode, SendOutcome};
use loom::plan_session::kind::SessionDir;
use loom::plan_session::model::{Item, NoticeKind, Origin, PermissionStatus, SessionState};
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

use super::{config, project, request, wait, Running};

fn wait_idle(r: &Running, id: &str) {
    thread::sleep(Duration::from_secs(1));
    let idle = || r.state(id) == Some(SessionState::Idle);
    wait("Idle", Duration::from_secs(300), idle);
}

fn send(r: &Running, id: &str, text: &str) -> SendOutcome {
    r.bridge.send(id, text.to_owned(), Vec::new()).unwrap()
}

fn any_item(r: &Running, wanted: impl Fn(&Item) -> bool) -> bool {
    r.items().iter().any(wanted)
}

/// Each spooled event's name and payload keys, in order.
fn spool_events(dir: &SessionDir) -> Vec<String> {
    let text = fs::read_to_string(dir.events()).unwrap_or_default();
    let lines = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok());
    let describe = |line: Value| {
        let keys = line["payload"]
            .as_object()
            .map(|o| o.keys().cloned().collect());
        let keys: Vec<String> = keys.unwrap_or_default();
        format!("{} {keys:?}", line["event"].as_str().unwrap_or("?"))
    };
    lines.map(describe).collect()
}

fn turn_ends(r: &Running) -> usize {
    let events = r.events.lock().unwrap();
    let ended = |e: &&BridgeEvent| matches!(e, BridgeEvent::TurnEnded { .. });
    events.iter().filter(ended).count()
}

/// The opening arrives by paste as the offer's user item, never on claude's argv.
fn open_chat(r: &Running, dir_of: impl Fn(&str) -> SessionDir) -> (String, SessionDir) {
    let opening = "Reply with the single word READY.";
    let start = request(opening, Some(Origin::Offer), false);
    let id = r.bridge.start_session(start).unwrap();
    let dir = dir_of(&id);
    wait_idle(r, &id);
    let offer = |item: &Item| {
        matches!(
            item,
            Item::User {
                origin: Some(Origin::Offer),
                ..
            }
        )
    };
    assert!(any_item(r, offer), "no opening user item with origin offer");
    let pid = read_record(&dir).unwrap().pid.unwrap();
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let on_argv = String::from_utf8_lossy(&cmdline).contains(opening);
    assert!(!on_argv, "the opening is on claude's argv");
    (id, dir)
}

/// A second message sent while the first works is queued, then delivered.
fn queue_round(r: &Running, id: &str) {
    send(r, id, "Name one prime number. One word.");
    let second = send(r, id, "Name one even number. One word.");
    assert!(matches!(second, SendOutcome::Queued { .. }), "{second:?}");
    let drained = || r.bridge.chat_snapshot(true).queue.is_empty();
    wait("the queue to drain", Duration::from_secs(300), drained);
    wait_idle(r, id);
}

/// `date` needs a permission; the holder allows it.
fn permission_round(r: &Running, id: &str) {
    send(
        r,
        id,
        "Run the shell command `date` with your Bash tool and tell me its output.",
    );
    let pending = || {
        let mut items = r.items().into_iter();
        items.find_map(|item| match item {
            Item::Permission {
                request_id,
                status: PermissionStatus::Pending,
                ..
            } => Some(request_id),
            _ => None,
        })
    };
    wait("the permission request", Duration::from_secs(120), || {
        pending().is_some()
    });
    let request_id = pending().unwrap();
    r.bridge
        .answer_permission(id, &request_id, true, None)
        .unwrap();
    wait_idle(r, id);
    let allowed = |item: &Item| {
        matches!(
            item,
            Item::Permission {
                status: PermissionStatus::Allowed,
                ..
            }
        )
    };
    assert!(any_item(r, allowed), "the permission item is not allowed");
}

/// The interrupted turn ends whether or not a Stop follows; prints which.
fn interrupt_round(r: &Running, id: &str, dir: &SessionDir) {
    send(
        r,
        id,
        "Count slowly from 1 to 300, one number per line, a sentence for each.",
    );
    let working = || r.state(id) == Some(SessionState::Working);
    wait("Working", Duration::from_secs(60), working);
    thread::sleep(Duration::from_secs(3));
    let (ends, spooled) = (turn_ends(r), spool_events(dir).len());
    r.bridge.interrupt(id).unwrap();
    wait(
        "the interrupted turn's end",
        Duration::from_secs(60),
        || turn_ends(r) > ends,
    );
    wait_idle(r, id);
    let after: Vec<String> = spool_events(dir).into_iter().skip(spooled).collect();
    let stop = after.iter().any(|line| line.starts_with("Stop "));
    let followed = if stop { "followed" } else { "did not follow" };
    println!("interrupt: a Stop {followed} the interrupt; events: {after:?}");
    let notice = |item: &Item| {
        matches!(
            item,
            Item::Notice {
                kind: NoticeKind::Interrupted,
                ..
            }
        )
    };
    assert!(any_item(r, notice), "no interrupted notice");
}

/// `/compact` is confirmed and the session returns to Idle; prints the event order.
fn compact_round(r: &Running, id: &str, dir: &SessionDir) {
    let spooled = spool_events(dir).len();
    send(r, id, "/compact");
    wait_idle(r, id);
    let order: Vec<String> = spool_events(dir).into_iter().skip(spooled).collect();
    println!("compact: {order:?}");
}

/// Prints `$TMPDIR` as the session sees it (plan 6's temporary directory rule).
fn tmpdir_round(r: &Running, id: &str) {
    let ask = "Run `echo $TMPDIR` with Bash, then write the word hi to a file named e2e.txt in \
               that directory. Reply with the directory.";
    send(r, id, ask);
    wait_idle(r, id);
    let mut texts = r.items().into_iter().filter_map(|item| match item {
        Item::Text { markdown, .. } => Some(markdown),
        _ => None,
    });
    println!("TMPDIR as the session sees it: {:?}", texts.next_back());
}

#[test]
#[ignore = "needs a logged-in Claude Code; the operator runs it with --include-ignored"]
#[serial]
fn operator_end_to_end_with_claude_code() {
    let claude = loom::claude::find_claude_path().expect("the operator run needs claude");
    let base = TempDir::new().unwrap();
    let base = base.path().canonicalize().unwrap();
    let (root, runtime) = project(&base);
    let git = Command::new("git").args(["init", "-q"]).arg(&root).status();
    assert!(git.unwrap().success(), "git init failed");
    let bound = Duration::from_secs(300);
    let r = Running::new(config(&root, &runtime, claude, bound));
    r.bridge.recover();
    let (id, dir) = open_chat(&r, |id| SessionDir::new(runtime.sessions.join(id)));
    queue_round(&r, &id);
    permission_round(&r, &id);
    interrupt_round(&r, &id, &dir);
    compact_round(&r, &id, &dir);
    tmpdir_round(&r, &id);
    r.bridge
        .end(&id, EndMode::FinalWrite, EndCause::Caller)
        .unwrap();
    wait("the end", Duration::from_secs(320), || {
        r.ended(&id).is_some()
    });
    println!(
        "ended: {:?}; events: {:?}",
        r.ended(&id),
        spool_events(&dir)
    );
    assert!(any_item(&r, is_closing), "no user item with origin closing");
    assert!(!dir.path().exists(), "the session directory survived");
    println!("items: {}", r.items().len());
    let ran = probe(&r, false);
    println!(
        "pasted slash command: {}",
        if ran { "ran" } else { "not run" }
    );
    if !ran {
        assert!(
            probe(&r, true),
            "the probe did not run from the launch line either"
        );
    }
}

/// The closing message the bridge pastes at a final write.
fn is_closing(item: &Item) -> bool {
    matches!(
        item,
        Item::User {
            origin: Some(Origin::Closing),
            ..
        }
    )
}

/// A session whose opening is the plugin's `/loom-plan:probe`; whether it ran.
fn probe(r: &Running, on_argv: bool) -> bool {
    let opening = "/loom-plan:probe doc/plans/PLAN-x.md";
    let start = request(opening, Some(Origin::Offer), on_argv);
    let id = r.bridge.start_session(start).unwrap();
    wait_idle(r, &id);
    let probed =
        |item: &Item| matches!(item, Item::Text { markdown, .. } if markdown.contains("PROBE-OK"));
    let ran = any_item(r, probed);
    r.bridge
        .end(&id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait("the probe's end", Duration::from_secs(60), || {
        r.ended(&id).is_some()
    });
    ran
}
