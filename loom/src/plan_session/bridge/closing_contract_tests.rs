//! Contract: the closing write runs only after the user sent a message; an offer chat the user
//! never wrote in ends at once, without pasting the closing message.

use serde_json::json;

use super::fake_host::{settle, wait_until, Harness, OPENING};
use super::{EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{Origin, SessionState};

#[test]
fn an_offer_chat_the_user_never_wrote_in_ends_without_a_closing_message() {
    let h = Harness::ready(|_| {});
    let mut request = h.request(Authoring, OPENING, false);
    request.opening_origin = Some(Origin::Offer);
    let id = h.bridge.start_session(request).unwrap();
    let s = h.sess(&id);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    h.working(&s);
    h.spool(&s, "Stop", json!({}));
    h.wait_state(&s, SessionState::Idle);

    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the end", || h.ended(&s.id).is_some());
    settle();
    assert_eq!(
        h.sent(&s),
        Vec::<String>::new(),
        "the closing message was pasted"
    );
    assert_eq!(h.ended(&s.id), Some(EndOutcome::TornDown));
}
