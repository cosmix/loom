//! Set aside through the production dispatch: the draft must stop reading live soon after the
//! command, whatever state the kickoff is in.

use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use super::drafts::{idle, set_aside_cmd, start_cmd, start_kickoff, stored, working};
use super::kickoff::{append, block, confirm};
use super::{fixture, send};
use crate::plan_server::chat::draft_cmd::SET_ASIDE_BOUND;
use crate::plan_server::chat::tests::{browser, ok_result, wait_for, wait_until, Fixture, Tab};
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::model::SessionState;
use crate::plan_session::scope::KickoffSize;

/// How long the notice may keep reading "still open" after the set-aside command is accepted.
const VISIBLE_WITHIN: Duration = Duration::from_secs(2);

#[test]
fn set_aside_of_an_unconfirmed_kickoff_in_a_running_turn_ends_the_draft_at_once() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = start_kickoff(&f, &tab, start_cmd("kickoff", Some("an idea"), Vec::new()));
    working(&f, &s);
    idle(&f, &s);
    // The user's message is delivered and its turn is still running when the kickoff is set aside.
    let typed = "one more thought";
    assert_eq!(f.command(&tab, send(typed, Vec::new())), ok_result());
    let pasted = wait_for("the message paste", || {
        let delivered = f.host.delivered_to(&s.socket).into_iter();
        delivered.rev().find(|pasted| pasted.contains(typed))
    });
    f.spool(&s, "UserPromptSubmit", json!({"prompt": pasted}));
    f.wait_state(&s, SessionState::Working);
    assert_eq!(stored(&f, &s.id)["live"], true, "the draft starts live");
    assert!(
        stored(&f, &s.id)["target"].is_null(),
        "nothing is confirmed"
    );

    assert_eq!(f.command(&tab, set_aside_cmd(&s.id)), ok_result());

    let deadline = Instant::now() + VISIBLE_WITHIN;
    while stored(&f, &s.id)["live"] == true && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        stored(&f, &s.id)["live"],
        false,
        "the draft still reads live {VISIBLE_WITHIN:?} after set aside: an unconfirmed kickoff \
         waits for its closing write instead of ending (ended events: {:?})",
        f.ended(&s.id)
    );
}

/// A feature kickoff confirmed at idle, its confirmation turn running.
fn confirmed_and_working(f: &Fixture, tab: &Tab) -> LaunchSpec {
    let s = start_kickoff(f, tab, start_cmd("kickoff", Some("an idea"), Vec::new()));
    working(f, &s);
    block(f, &s, tab, json!({"name": "fresh", "problem": "P"}));
    idle(f, &s);
    assert_eq!(
        f.command(tab, confirm(KickoffSize::Feature, "fresh")),
        ok_result()
    );
    let pasted = wait_for("the confirmation paste", || {
        let delivered = f.host.delivered_to(&s.socket).into_iter();
        delivered.rev().find(|text| text.contains("confirmed the"))
    });
    f.spool(&s, "UserPromptSubmit", json!({"prompt": pasted}));
    f.wait_state(&s, SessionState::Working);
    s
}

/// The "Stopped" notices an interrupt posts to the chat, as the tab received them.
fn stopped_notices(tab: &Tab) -> usize {
    let frames = tab.frames().into_iter().map(|frame| frame.to_string());
    frames.filter(|frame| frame.contains("\"Stopped")).count()
}

/// The interrupted turn ends as a real one does: the transcript records the interruption. The
/// session reads Ending by now, so the idle underneath shows only as the closing paste.
fn interrupted(f: &Fixture, s: &LaunchSpec) {
    let text = "[Request interrupted by user]";
    let content = json!([{"type": "text", "text": text}]);
    append(
        f,
        s,
        json!({"type": "user", "message": {"role": "user", "content": content}}),
    );
}

fn closing_pasted(f: &Fixture, s: &LaunchSpec) -> bool {
    let closing = crate::planning::guidance::CLOSING_MESSAGE;
    f.host
        .delivered_to(&s.socket)
        .iter()
        .any(|text| text == closing)
}

/// Set aside of a confirmed kickoff in a running turn: the turn is interrupted, the closing
/// message waits for the idle that interrupt causes, and its turn ends the session.
#[test]
fn set_aside_of_a_confirmed_kickoff_in_a_running_turn_interrupts_it_then_closes() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = confirmed_and_working(&f, &tab);
    assert_eq!(stopped_notices(&tab), 0);

    assert_eq!(f.command(&tab, set_aside_cmd(&s.id)), ok_result());

    wait_until("the interrupt", || stopped_notices(&tab) == 1);
    thread::sleep(Duration::from_millis(200));
    assert!(
        !closing_pasted(&f, &s),
        "the closing message went into the running turn"
    );
    assert!(f.ended(&s.id).is_empty(), "the closing write is waited for");
    interrupted(&f, &s);
    f.finish_final_write(&s);
    wait_until("the end after the closing turn", || {
        !f.ended(&s.id).is_empty()
    });
    wait_until("the draft going not live", || {
        stored(&f, &s.id)["live"] == false
    });
}

/// A second set aside while the closing turn runs only re-bounds the end: the closing turn is
/// not interrupted. (The session reads Ending then, which the bridge's interrupt refuses too;
/// `close_within` skips the interrupt once an end is planned, whatever the state reads.)
#[test]
fn a_repeated_set_aside_leaves_the_closing_turn_running() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = confirmed_and_working(&f, &tab);
    assert_eq!(f.command(&tab, set_aside_cmd(&s.id)), ok_result());
    wait_until("the interrupt", || stopped_notices(&tab) == 1);
    interrupted(&f, &s);
    wait_until("the closing message", || closing_pasted(&f, &s));
    let closing = crate::planning::guidance::CLOSING_MESSAGE;
    f.spool(&s, "UserPromptSubmit", json!({"prompt": closing}));
    thread::sleep(Duration::from_millis(200));

    assert_eq!(f.command(&tab, set_aside_cmd(&s.id)), ok_result());

    thread::sleep(Duration::from_millis(300));
    assert_eq!(stopped_notices(&tab), 1, "the closing turn was interrupted");
    f.spool(&s, "Stop", json!({}));
    wait_until("the end after the closing turn", || {
        !f.ended(&s.id).is_empty()
    });
}

/// A confirmed set aside plans its end with [`SET_ASIDE_BOUND`]: a plain `end` would leave the
/// closing write the server's `final_write_bound` (600 s in production) instead.
#[test]
fn a_confirmed_set_aside_bounds_its_closing_write_by_the_set_aside_bound() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = confirmed_and_working(&f, &tab);

    assert_eq!(f.command(&tab, set_aside_cmd(&s.id)), ok_result());

    assert_eq!(f.bridge.planned_end_bound(&s.id), Some(SET_ASIDE_BOUND));
}
