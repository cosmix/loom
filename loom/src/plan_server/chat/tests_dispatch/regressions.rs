//! Scope-gated kickoff tracking and retryable confirmation failures.

use serde_json::json;

use super::super::{lock, ChatState};
use super::kickoff::{append, launch};
use super::{browser, fixture, ok_result, opening, received_frames, refused, wait_until};
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::bridge::{EndCause, EndMode};
use crate::plan_session::model::SessionState;
use crate::plan_session::scope::{self, KickoffGrant, KickoffSize, SessionScope};

fn confirm() -> ChatCommand {
    ChatCommand::ConfirmKickoff {
        client_id: Some("c1".into()),
        size: KickoffSize::Feature,
        name: "fresh".into(),
    }
}

#[test]
fn a_failed_confirmation_send_restores_the_scope_and_can_be_confirmed_again() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "kickoff", None);
    let previous = scope::load(&s.dir).unwrap();
    // Keep the opening pending so the bridge queues every send.
    for _ in 0..20 {
        f.bridge.send(&s.id, "waiting".into(), Vec::new()).unwrap();
    }

    assert_eq!(
        f.command(&tab, confirm()),
        refused("at most 20 messages can wait")
    );
    assert_eq!(scope::load(&s.dir).unwrap(), previous);
    assert!(!received_frames(&f, &tab)
        .iter()
        .any(|frame| frame["type"] == "kickoff"));

    let queued = f.bridge.chat_snapshot(true).queue[0].id.clone();
    f.bridge.withdraw(&s.id, &queued).unwrap();
    assert_eq!(f.command(&tab, confirm()), ok_result());
    let SessionScope::Kickoff(kickoff) = scope::load(&s.dir).unwrap() else {
        panic!("kickoff scope missing");
    };
    assert_eq!(
        kickoff.grant,
        Some(KickoffGrant {
            size: KickoffSize::Feature,
            name: "fresh".into(),
        })
    );
}

#[test]
fn open_chat_assistant_text_does_not_publish_or_replay_kickoff_shapes() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = f.start(&tab);
    scope::write(&s.dir, &SessionScope::Open).unwrap();
    opening(&f, &s);
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "opening"}));
    f.wait_state(&s, SessionState::Working);
    let markdown = "```loom-kickoff\n{\"problem\":\"Ordinary conversation\"}\n```";

    append(
        &f,
        &s,
        json!({"type": "assistant", "message": {
            "id": "ordinary-text", "role": "assistant",
            "content": [{"type": "text", "text": markdown}], "stop_reason": "end_turn"
        }}),
    );
    wait_until("the ordinary text delta", || {
        tab.frames().iter().any(|frame| {
            frame["type"] == "chat"
                && frame["op"] == "item_added"
                && frame["item"]["markdown"] == markdown
        })
    });

    assert!(!received_frames(&f, &tab)
        .iter()
        .any(|frame| frame["type"] == "kickoff"));
    assert!(!f
        .connect(&browser())
        .opened
        .iter()
        .any(|frame| frame["type"] == "kickoff"));
    let state = f.ctx.extensions().get::<ChatState>().unwrap();
    assert_eq!(lock(&state.kickoff_scopes).get(&s.id), Some(&false));
    f.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the ended ordinary chat", || !f.ended(&s.id).is_empty());
    assert!(!lock(&state.kickoff_scopes).contains_key(&s.id));
}
