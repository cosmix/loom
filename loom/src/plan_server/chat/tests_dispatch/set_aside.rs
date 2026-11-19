//! Set aside through the production dispatch: the draft must stop reading live soon after the
//! command, whatever state the kickoff is in.

use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use super::drafts::{idle, set_aside_cmd, start_cmd, start_kickoff, stored, working};
use super::{fixture, send};
use crate::plan_server::chat::tests::{browser, ok_result, wait_for};
use crate::plan_session::model::SessionState;

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
