//! The operator's run with a logged-in Claude Code: the opening by paste, a queued second
//! message, a permission round, an interrupt, `/compact`, the session's temporary directory,
//! the final write, and whether a pasted plugin slash command runs. Prints what the installed
//! Claude Code does where common.md assumes it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use loom::fs::permissions::untrust_worktree;
use loom::plan_session::bridge::registry::read_record;
use loom::plan_session::bridge::{EndCause, EndMode, SendOutcome};
use loom::plan_session::kind::SessionDir;
use loom::plan_session::model::{
    Delivery, Item, NoticeKind, Origin, PermissionStatus, SessionState,
};
use serial_test::serial;
use tempfile::TempDir;

use super::diagnose::{
    await_turn_ends, check, preserve, spool_events, spooled_after, stops_in, turn_ends,
    turn_starts, until, within,
};
use super::{config, project, request, Running};

/// Removes the temp project's trust entry from the real `~/.claude.json` when the run ends.
struct TrustGuard<'a>(&'a Path);

impl Drop for TrustGuard<'_> {
    fn drop(&mut self) {
        let _ = untrust_worktree(self.0);
    }
}

/// Waits for Idle; a timeout preserves the session's evidence. The state is Idle from the
/// previous `Stop` (or the startup) until the new message's `UserPromptSubmit`, and the
/// bridge pastes on a later tick: a caller first waits for evidence of its own turn.
fn wait_idle(r: &Running, id: &str, dir: &SessionDir) {
    thread::sleep(secs(1));
    let idle = || r.state(id) == Some(SessionState::Idle);
    until(r, id, dir, "Idle", secs(300), idle);
}

fn send(r: &Running, id: &str, text: &str) -> SendOutcome {
    r.bridge.send(id, text.to_owned(), Vec::new()).unwrap()
}

fn any_item(r: &Running, wanted: impl Fn(&Item) -> bool) -> bool {
    r.items().iter().any(wanted)
}

/// Allows every permission request still pending and prints its tool. The authoring sandbox
/// does not auto-allow Bash, so a round that runs a command may raise several. `r.items()` is
/// a copy: the bridge is called with no lock held.
fn allow_pending(r: &Running, id: &str) {
    let pending = r.items().into_iter().filter_map(|item| match item {
        Item::Permission {
            request_id,
            tool,
            status: PermissionStatus::Pending,
            ..
        } => Some((request_id, tool)),
        _ => None,
    });
    for (request_id, tool) in pending {
        // A card that closed since the snapshot (a Stop, a timeout) is refused.
        match r.bridge.answer_permission(id, &request_id, true, None) {
            Ok(()) => println!("permission: allowed {tool} ({request_id})"),
            Err(error) => println!("permission: {tool} ({request_id}) not answered: {error}"),
        }
    }
}

/// The opening arrives by paste as the offer's user item, never on claude's argv.
fn open_chat(r: &Running, dir_of: impl Fn(&str) -> SessionDir) -> (String, SessionDir) {
    let opening = "Reply with the single word READY.";
    let start = request(opening, Some(Origin::Offer), false);
    let id = r.bridge.start_session(start).unwrap();
    let dir = dir_of(&id);
    let offer = |item: &Item| {
        matches!(
            item,
            Item::User {
                origin: Some(Origin::Offer),
                delivery: Delivery::Delivered,
                ..
            }
        )
    };
    let arrived = || any_item(r, offer);
    until(r, &id, &dir, "the opening's delivery", secs(120), arrived);
    let ended = || turn_ends(r, &id) > 0;
    until(r, &id, &dir, "the opening turn's end", secs(300), ended);
    wait_idle(r, &id, &dir);
    let pid = read_record(&dir).ok().and_then(|record| record.pid);
    check(r, &id, &dir, pid.is_some(), "the session record has no pid");
    let cmdline = fs::read(format!("/proc/{}/cmdline", pid.unwrap())).unwrap_or_default();
    let on_argv = String::from_utf8_lossy(&cmdline).contains(opening);
    check(r, &id, &dir, !on_argv, "the opening is on claude's argv");
    (id, dir)
}

/// Whether `item` is the delivered user message containing `text`.
fn delivered(item: &Item, text: &str) -> bool {
    matches!(item, Item::User { text: t, delivery: Delivery::Delivered, .. } if t.contains(text))
}

/// A second message sent while the first works is queued, then delivered. Returns the turn
/// ends expected once both turns have ended.
fn queue_round(r: &Running, id: &str, dir: &SessionDir, before: usize) -> usize {
    let first = "Name one prime number. One word.";
    let second = "Name one even number. One word.";
    send(r, id, first);
    let outcome = send(r, id, second);
    let queued = matches!(outcome, SendOutcome::Queued { .. });
    let what = format!("the second message was not queued: {outcome:?}");
    check(r, id, dir, queued, &what);
    // The second message is pasted when the first turn ends, so its 60 s start there. The
    // queue empties when the bridge pops it, before its paste.
    await_turn_ends(r, id, dir, before + 1, secs(300));
    let sent = || any_item(r, |item| delivered(item, second));
    until(r, id, dir, "the second message's delivery", secs(60), sent);
    await_turn_ends(r, id, dir, before + 2, secs(300));
    wait_idle(r, id, dir);
    before + 2
}

/// `date` needs a permission; the holder allows it. Returns the turn ends expected once the
/// turn has ended.
fn permission_round(r: &Running, id: &str, dir: &SessionDir, before: usize) -> usize {
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
    let asked = || pending().is_some();
    until(r, id, dir, "the permission request", secs(120), asked);
    let request_id = pending().unwrap();
    r.bridge
        .answer_permission(id, &request_id, true, None)
        .unwrap();
    // The turn may raise more requests; none is left unanswered.
    let finished = || {
        allow_pending(r, id);
        turn_ends(r, id) > before
    };
    until(r, id, dir, "the date turn's end", secs(300), finished);
    wait_idle(r, id, dir);
    let allowed = |item: &Item| {
        matches!(
            item,
            Item::Permission {
                status: PermissionStatus::Allowed,
                ..
            }
        )
    };
    let is_allowed = any_item(r, allowed);
    check(r, id, dir, is_allowed, "the permission item is not allowed");
    before + 1
}

/// The interrupted turn ends whether or not a Stop follows; prints which. Returns the turn
/// ends expected once it has ended.
fn interrupt_round(r: &Running, id: &str, dir: &SessionDir, before: usize) -> usize {
    send(
        r,
        id,
        "Count slowly from 1 to 300, one number per line, a sentence for each.",
    );
    let working = || r.state(id) == Some(SessionState::Working);
    until(r, id, dir, "Working", secs(60), working);
    thread::sleep(secs(3));
    let spooled = spool_events(dir).len();
    r.bridge.interrupt(id).unwrap();
    await_turn_ends(r, id, dir, before + 1, secs(60));
    wait_idle(r, id, dir);
    // A Stop can be spooled after the Idle the interrupt leaves: give it 5 s before the
    // report says "neither".
    let stopped = || !stops_in(&spooled_after(dir, spooled)).is_empty();
    within(secs(5), stopped);
    let after = spooled_after(dir, spooled);
    let stops = stops_in(&after);
    let followed = if stops.is_empty() {
        "neither Stop nor StopFailure followed".to_owned()
    } else {
        format!("{} followed", stops.join(" and "))
    };
    println!("interrupt: {followed} the interrupt; events: {after:?}");
    let notice = |item: &Item| {
        matches!(
            item,
            Item::Notice {
                kind: NoticeKind::Interrupted,
                ..
            }
        )
    };
    check(r, id, dir, any_item(r, notice), "no interrupted notice");
    before + 1
}

/// `/compact` is confirmed and the session returns to Idle; prints the event order. Claude
/// Code may or may not open a turn for it: the turns that started raise the expected total,
/// and the next round's wait covers their ends.
fn compact_round(r: &Running, id: &str, dir: &SessionDir, before: usize) -> usize {
    let (spooled, started) = (spool_events(dir).len(), turn_starts(r, id));
    send(r, id, "/compact");
    // The transcript's compact boundary becomes the notice.
    let compacted = |item: &Item| {
        matches!(
            item,
            Item::Notice {
                kind: NoticeKind::Compacted,
                ..
            }
        )
    };
    let confirmed = || any_item(r, compacted);
    until(r, id, dir, "the compacted notice", secs(300), confirmed);
    wait_idle(r, id, dir);
    // A turn's start is published long before the Idle that ends it; its end may not be yet.
    let turns = turn_starts(r, id) - started;
    let order = spooled_after(dir, spooled);
    println!("compact: {order:?}; turns started: {turns}");
    before + turns
}

/// Prints `$TMPDIR` as the session sees it (plan 6's temporary directory rule). `before` is
/// the turn ends expected from earlier rounds: the wait needs one more, this turn's own. The
/// sandbox may ask permission for `echo` and for the write, so each request is allowed.
fn tmpdir_round(r: &Running, id: &str, dir: &SessionDir, before: usize) {
    let ask = "Run `echo $TMPDIR` with Bash, then write the word hi to a file named e2e.txt in \
               that directory. Reply with the directory.";
    send(r, id, ask);
    let ended = || {
        allow_pending(r, id);
        turn_ends(r, id) > before
    };
    until(r, id, dir, "the TMPDIR turn's end", secs(300), ended);
    wait_idle(r, id, dir);
    let mut texts = r.items().into_iter().filter_map(|item| match item {
        Item::Text { markdown, .. } => Some(markdown),
        _ => None,
    });
    println!("TMPDIR as the session sees it: {:?}", texts.next_back());
}

/// The first line of `program arg`'s output, for the operator's memory note.
fn version_line(program: &Path, arg: &str) -> String {
    let shown = match Command::new(program).arg(arg).output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines().next().unwrap_or_default().to_owned()
        }
        Err(error) => format!("cannot run: {error}"),
    };
    format!("{} {arg}: {shown}", program.display())
}

fn secs(count: u64) -> Duration {
    Duration::from_secs(count)
}

/// The rounds after the opening. `ended` is the running total of turn ends the chat must
/// show, never a fresh count: a session reads Idle before its turn's `TurnEnded` is
/// published, so a count taken after `wait_idle` can miss the turn just ended. Each round
/// waits for the previous total plus its own turns, and returns the new total.
fn rounds(r: &Running, id: &str, dir: &SessionDir) {
    let ended = 1; // the opening's turn
    let ended = queue_round(r, id, dir, ended);
    let ended = permission_round(r, id, dir, ended);
    let ended = interrupt_round(r, id, dir, ended);
    let ended = compact_round(r, id, dir, ended);
    tmpdir_round(r, id, dir, ended);
}

#[test]
#[ignore = "needs a logged-in Claude Code; the operator runs it with --include-ignored"]
#[serial]
fn operator_end_to_end_with_claude_code() {
    let claude = loom::claude::find_claude_path().expect("the operator run needs claude");
    println!("{}", version_line(&claude, "--version"));
    println!("{}", version_line(Path::new("tmux"), "-V"));
    let base = TempDir::new().unwrap();
    let base = base.path().canonicalize().unwrap();
    let (root, runtime) = project(&base);
    let git = Command::new("git").args(["init", "-q"]).arg(&root).status();
    assert!(git.unwrap().success(), "git init failed");
    let bound = secs(300);
    // The logged-in Claude Code reads the real HOME; the launch trusts the temp project there
    // and the guard removes that entry. Declared before `r` so it drops after the bridge stops.
    let home = PathBuf::from(std::env::var_os("HOME").expect("the operator run needs HOME"));
    let _trust = TrustGuard(&root);
    let r = Running::new(config(&root, &runtime, claude, &home, bound));
    r.bridge.recover();
    let dir_of = |id: &str| SessionDir::new(runtime.sessions.join(id));
    let (id, dir) = open_chat(&r, dir_of);
    rounds(&r, &id, &dir);
    r.bridge
        .end(&id, EndMode::FinalWrite, EndCause::Caller)
        .unwrap();
    let ended = || r.ended(&id).is_some();
    until(&r, &id, &dir, "the end", secs(320), ended);
    println!(
        "ended: {:?}; events: {:?}",
        r.ended(&id),
        spool_events(&dir)
    );
    let closing = any_item(&r, is_closing);
    check(&r, &id, &dir, closing, "no user item with origin closing");
    let survived = dir.path().exists();
    check(&r, &id, &dir, !survived, "the session directory survived");
    println!("items: {}", r.items().len());
    let ran = probe(&r, dir_of, false);
    println!(
        "pasted slash command: {}",
        if ran { "ran" } else { "not run" }
    );
    if !ran {
        probe(&r, dir_of, true);
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

/// Waits for the probe session's own turn to end; the startup Idle says nothing about the
/// opening. A pasted probe with no turn end within the bound counts as not run (Claude Code
/// may handle an unknown slash command locally), so it keeps the pane instead of failing;
/// the argv probe must run.
fn await_probe_turn(r: &Running, id: &str, dir: &SessionDir, on_argv: bool) {
    let ended = || turn_ends(r, id) > 0;
    if on_argv {
        until(r, id, dir, "the argv probe's turn end", secs(300), ended);
    } else if !within(secs(120), ended) {
        let saved = preserve("pasted probe", r, id, dir);
        println!(
            "pasted probe: no turn ended; diagnostics in {}",
            saved.display()
        );
    }
}

/// A session whose opening is the plugin's `/loom-plan:probe`; whether it ran. A probe on
/// the launch line is the last resort, so it must run.
fn probe(r: &Running, dir_of: impl Fn(&str) -> SessionDir, on_argv: bool) -> bool {
    let opening = "/loom-plan:probe doc/plans/PLAN-x.md";
    let start = request(opening, Some(Origin::Offer), on_argv);
    let id = r.bridge.start_session(start).unwrap();
    let dir = dir_of(&id);
    await_probe_turn(r, &id, &dir, on_argv);
    wait_idle(r, &id, &dir);
    let probed =
        |item: &Item| matches!(item, Item::Text { markdown, .. } if markdown.contains("PROBE-OK"));
    let ran = any_item(r, probed);
    if on_argv {
        let what = "the probe did not run from the launch line either";
        check(r, &id, &dir, ran, what);
    }
    r.bridge
        .end(&id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    let ended = || r.ended(&id).is_some();
    until(r, &id, &dir, "the probe's end", secs(60), ended);
    ran
}
