//! Contract: a start command needs a message or an offer; an empty one launches nothing.

use super::tests::{browser, Fixture};
use crate::plan_server::wire::chat::ChatCommand;

#[test]
fn a_start_with_no_offer_and_no_text_launches_nothing() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    // Built inline: the shared start() helper is not the contract's to pin.
    let empty = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        feature: None,
        plan: None,
        text: None,
        attachments: Vec::new(),
    };
    let result = f.command(&tab, empty);
    assert_eq!(result["ok"], false, "an empty start was accepted: {result}");
    assert_eq!(
        f.bridge.chat_session_id(),
        None,
        "an empty start launched a session"
    );
    assert!(
        f.ctx.authoring().holder().is_none(),
        "an empty start took authoring"
    );
}
