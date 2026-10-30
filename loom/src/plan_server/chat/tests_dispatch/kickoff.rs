//! Kickoff starts, holder confirmation and the taking-shape frames.

use std::fs::OpenOptions;
use std::io::Write;

use serde_json::{json, Value};

use super::super::tests::{browser, wait_for, wait_until, Fixture, Tab};
use super::super::{publish, ChatState};
use super::{fixture, ok_result, opening, received_frames, record_guidance, refused};
use crate::plan_server::authoring::{Refusal, Take};
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::registry::read_record;
use crate::plan_session::bridge::{EndCause, EndMode};
use crate::plan_session::model::{ChatDelta, Item, SessionState};
use crate::plan_session::scope::{self, KickoffGrant, KickoffSize, SessionScope};

pub(super) fn offer(id: &str, feature: Option<&str>) -> ChatCommand {
    ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: Some(id.into()),
        feature: feature.map(str::to_owned),
        plan: None,
        text: None,
        attachments: Vec::new(),
    }
}

fn confirm(size: KickoffSize, name: &str) -> ChatCommand {
    ChatCommand::ConfirmKickoff {
        client_id: Some("c1".into()),
        size,
        name: name.into(),
    }
}

pub(super) fn launch(f: &Fixture, tab: &Tab, id: &str, feature: Option<&str>) -> LaunchSpec {
    assert_eq!(f.command(tab, offer(id, feature)), ok_result());
    let id = wait_for("the chat", || f.bridge.chat_session_id());
    f.launched(&id)
}

pub(super) fn append(f: &Fixture, s: &LaunchSpec, mut record: Value) {
    let path = f.bridge.transcripts(&s.id).unwrap().main.unwrap();
    record["uuid"] = json!(uuid::Uuid::new_v4().to_string());
    record["sessionId"] = json!(s.id);
    record["timestamp"] = json!(chrono::Utc::now().to_rfc3339());
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    writeln!(file, "{record}").unwrap();
}

pub(super) fn shape(f: &Fixture, s: &LaunchSpec, tab: &Tab, problem: &str) -> Value {
    let markdown = format!(
        "```loom-kickoff\n{}\n```",
        json!({"size": "feature", "name": "fresh", "proposal": true, "problem": problem})
    );
    append(
        f,
        s,
        json!({"type": "assistant", "message": {
            "id": "proposal", "role": "assistant",
            "content": [{"type": "text", "text": markdown}], "stop_reason": "end_turn"
        }}),
    );
    wait_for("the kickoff frame", || {
        tab.frames()
            .into_iter()
            .rev()
            .find(|frame| frame["type"] == "kickoff" && frame["shape"]["problem"] == problem)
    })
}

#[test]
fn both_kickoff_offers_write_an_unarmed_scope_without_team_environment() {
    for id in ["kickoff", "kickoff-small"] {
        let f = fixture(false);
        let tab = f.connect(&browser());
        let s = launch(&f, &tab, id, None);

        let SessionScope::Kickoff(kickoff) = scope::load(&s.dir).unwrap() else {
            panic!("kickoff scope missing");
        };
        assert_eq!(kickoff.grant, None);
        assert!(kickoff.existing.contains(&"doc/product/demo".into()));
        assert!(s.extra_env.is_empty());
    }
}

#[test]
fn an_ordinary_offer_writes_no_scope_and_sets_no_team_environment() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    record_guidance(&f);
    let s = launch(&f, &tab, "refine:doc/product/demo/index.md", Some("demo"));

    assert!(!s.dir.scope().exists());
    assert_eq!(scope::load(&s.dir).unwrap(), SessionScope::Open);
    assert!(s.extra_env.is_empty());
}

#[test]
fn confirming_before_a_chat_exists_refuses_without_starting_one() {
    let f = fixture(false);
    let holder = browser();
    let tab = f.connect(&holder);
    f.ctx.require_authoring(&holder, Take::TakeIfFree).unwrap();

    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "fresh")),
        refused("no chat is running")
    );
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn a_non_holder_cannot_confirm_the_kickoff() {
    let f = fixture(false);
    let holder = f.connect(&browser());
    let observer = f.connect(&browser());
    let s = launch(&f, &holder, "kickoff", None);
    let error = f.ctx.refusal_message(&Refusal::NotHolder {
        holder: holder.conn.public_id.clone(),
    });

    assert_eq!(
        f.command(&observer, confirm(KickoffSize::Feature, "fresh")),
        refused(error)
    );
    let SessionScope::Kickoff(kickoff) = scope::load(&s.dir).unwrap() else {
        panic!("kickoff scope missing");
    };
    assert_eq!(kickoff.grant, None);
}

fn ready_proposal(f: &Fixture, tab: &Tab) -> LaunchSpec {
    let s = launch(f, tab, "kickoff", None);
    opening(f, &s);
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "opening"}));
    f.wait_state(&s, SessionState::Working);
    shape(f, &s, tab, "A fresh feature");
    f.spool(&s, "Stop", json!({}));
    f.wait_state(&s, SessionState::Idle);
    s
}

#[test]
fn confirmation_arms_one_target_sends_one_message_and_publishes_the_target() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = ready_proposal(&f, &tab);
    let expected =
        "The user confirmed the feature \"fresh\". You may now create doc/product/fresh/index.md";

    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "fresh")),
        ok_result()
    );

    wait_until("the confirmation paste", || {
        f.host.delivered_to(&s.socket).len() == 2
    });
    assert_eq!(f.host.delivered_to(&s.socket)[1], expected);
    let SessionScope::Kickoff(kickoff) = scope::load(&s.dir).unwrap() else {
        panic!("kickoff scope missing");
    };
    assert_eq!(
        kickoff.grant,
        Some(KickoffGrant {
            size: KickoffSize::Feature,
            name: "fresh".into()
        })
    );
    assert!(received_frames(&f, &tab).iter().any(|frame| {
        frame["type"] == "kickoff" && frame["target"] == "doc/product/fresh/index.md"
    }));
    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "another")),
        refused("This kickoff already has a confirmed target.")
    );
    assert_eq!(f.host.delivered_to(&s.socket).len(), 2);
}

#[test]
fn confirmation_makes_the_named_feature_or_plan_the_sessions_owner() {
    for (size, feature, plan) in [
        (KickoffSize::Feature, Some("fresh"), None),
        (KickoffSize::Plan, None, Some("fresh")),
    ] {
        let f = fixture(false);
        let tab = f.connect(&browser());
        let s = ready_proposal(&f, &tab);

        assert_eq!(f.command(&tab, confirm(size, "fresh")), ok_result());

        let session = f.bridge.chat_snapshot(true).session.unwrap();
        assert_eq!(session.feature.as_deref(), feature);
        assert_eq!(session.plan.as_deref(), plan);
        wait_until("the rewritten record", || {
            read_record(&s.dir)
                .is_ok_and(|r| r.feature.as_deref() == feature && r.plan.as_deref() == plan)
        });
        wait_until("the published view", || {
            received_frames(&f, &tab).iter().any(|frame| {
                frame["op"] == "session"
                    && frame["session"]["feature"].as_str() == feature
                    && frame["session"]["plan"].as_str() == plan
            })
        });
    }
}

#[test]
fn a_standalone_plan_confirmation_names_its_document_and_brief_bundle() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = ready_proposal(&f, &tab);

    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Plan, "fresh")),
        ok_result()
    );

    wait_until("the confirmation paste", || {
        f.host.delivered_to(&s.socket).len() == 2
    });
    assert_eq!(f.host.delivered_to(&s.socket)[1],
        "The user confirmed the standalone plan \"fresh\". You may now create doc/plans/PLAN-fresh.md and files inside doc/plans/briefs/fresh/");
    assert!(scope::load(&s.dir)
        .unwrap()
        .allows_write(std::path::Path::new(
            "doc/plans/briefs/fresh/implementation.md"
        ))
        .is_ok());
}

#[test]
fn an_existing_feature_cannot_be_confirmed() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "kickoff", None);

    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "demo")),
        refused("The target already exists at doc/product/demo.")
    );
    let SessionScope::Kickoff(kickoff) = scope::load(&s.dir).unwrap() else {
        panic!("kickoff scope missing");
    };
    assert_eq!(kickoff.grant, None);
    assert!(f.host.delivered_to(&s.socket).is_empty());
}

#[test]
fn confirmation_in_an_ordinary_chat_is_refused() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    f.start(&tab);

    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "fresh")),
        refused("This session is not a kickoff session.")
    );
}

#[test]
fn confirmation_is_refused_when_read_only_or_stopping() {
    for read_only in [true, false] {
        let f = fixture(read_only);
        let tab = f.connect(&browser());
        let expected = if read_only {
            "loom plan is in read-only mode"
        } else {
            f.ctx.shutdown().abort_all();
            "loom plan is stopping"
        };

        assert_eq!(
            f.command(&tab, confirm(KickoffSize::Feature, "fresh")),
            refused(expected)
        );
        assert!(f.bridge.chat_session_id().is_none());
    }
}

#[test]
fn text_proposals_are_broadcast_replayed_on_open_and_cleared_at_session_end() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "kickoff", None);
    opening(&f, &s);
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "opening"}));
    f.wait_state(&s, SessionState::Working);

    let view = shape(&f, &s, &tab, "First proposal");
    assert_eq!(view["session"], s.id);
    assert!(view["target"].is_null());
    assert!(received_frames(&f, &tab).iter().any(|frame| {
        frame["type"] == "chat" && frame["op"] == "item_added" && frame["item"]["type"] == "text"
    }));
    let opened = f.connect(&browser()).opened;
    assert!(opened.iter().any(|frame| frame == &view));
    let updated = shape(&f, &s, &tab, "Updated proposal");
    assert!(f
        .connect(&browser())
        .opened
        .iter()
        .any(|frame| frame == &updated));

    f.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the ended session", || !f.ended(&s.id).is_empty());
    let opened = f.connect(&browser()).opened;
    assert!(!opened.iter().any(|frame| frame["type"] == "kickoff"));
}

#[test]
fn replacing_a_text_item_publishes_the_latest_kickoff_shape() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = ready_proposal(&f, &tab);
    let state = f.ctx.extensions().get::<ChatState>().unwrap();
    let turn_id = f
        .bridge
        .chat_snapshot(true)
        .turns
        .last()
        .unwrap()
        .id
        .clone();
    let delta = ChatDelta::ItemReplaced {
        turn_id,
        item: Item::Text {
            id: "replacement".into(),
            markdown: "```loom-kickoff\n{\"problem\":\"Replacement\"}\n```".into(),
        },
    };

    publish::delta(&f.ctx, &state, &f.bridge, &delta);

    let frames = received_frames(&f, &tab);
    assert!(frames.iter().any(|frame| {
        frame["type"] == "kickoff"
            && frame["session"] == s.id
            && frame["shape"]["problem"] == "Replacement"
    }));
    assert!(frames.iter().any(|frame| {
        frame["type"] == "chat"
            && frame["op"] == "item_replaced"
            && frame["item"]["id"] == "replacement"
    }));
}
