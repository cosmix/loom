//! Kickoff drafts through the production lifecycle: start, shapes, sends, rename, end, restart,
//! landing and continuation. Every file read here was written by the server.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use super::super::drafts;
use super::super::tests::{browser, ok_result, wait_for, wait_until, Fixture, Tab};
use super::super::ChatState;
use super::files::{file_input, store_file, ID};
use super::kickoff::{block, confirm};
use super::{fixture, opening, record_guidance, send};
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_server::wire::kickoff::KickoffShape;
use crate::plan_session::attachments::stored::read_sidecar;
use crate::plan_session::attachments::AttachmentInput;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::{EndCause, EndMode};
use crate::plan_session::model::SessionState;
use crate::plan_session::scope::KickoffSize;

pub(super) const OLD: &str = "11111111-1111-4111-8111-111111111111";

pub(super) fn draft_file(f: &Fixture, session: &str) -> PathBuf {
    let dir = f.ctx.runtime().dir.join("kickoffs");
    dir.join(format!("{session}.json"))
}

pub(super) fn stored(f: &Fixture, session: &str) -> Value {
    let text = fs::read_to_string(draft_file(f, session)).expect("a draft file");
    serde_json::from_str(&text).expect("a draft that parses")
}

/// A draft file as an earlier server run left it, ended; `patch` replaces top-level keys.
pub(super) fn write_draft(f: &Fixture, session: &str, patch: Value) -> PathBuf {
    let mut draft = json!({
        "session": session,
        "started_at": "2026-11-04T10:47:11Z",
        "updated_at": "2026-11-04T10:50:00Z",
        "messages": ["first idea"],
        "shape": {"size": "feature", "name": "agent-name", "proposal": false,
            "problem": "A problem", "decisions": ["Use one VM."],
            "questions": ["Which hosts?"], "sections": []},
        "attachments": [],
        "name_override": null,
        "target": null,
        "live": false
    });
    for (key, value) in patch.as_object().into_iter().flatten() {
        draft[key] = value.clone();
    }
    let path = draft_file(f, session);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&draft).unwrap()).unwrap();
    path
}

pub(super) fn start_cmd(
    offer: &str,
    text: Option<&str>,
    attachments: Vec<AttachmentInput>,
) -> ChatCommand {
    ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: Some(offer.into()),
        feature: None,
        plan: None,
        text: text.map(str::to_owned),
        attachments,
    }
}

pub(super) fn rename_cmd(name: &str) -> ChatCommand {
    ChatCommand::RenameKickoff {
        client_id: Some("c1".into()),
        name: name.into(),
    }
}

pub(super) fn set_aside_cmd(session: &str) -> ChatCommand {
    ChatCommand::SetAsideKickoff {
        client_id: Some("c1".into()),
        session: session.into(),
    }
}

pub(super) fn continue_cmd(session: &str) -> ChatCommand {
    ChatCommand::ContinueKickoff {
        client_id: Some("c1".into()),
        session: session.into(),
    }
}

pub(super) fn discard_cmd(session: &str) -> ChatCommand {
    ChatCommand::DiscardKickoff {
        client_id: Some("c1".into()),
        session: session.into(),
    }
}

pub(super) fn start_kickoff(f: &Fixture, tab: &Tab, cmd: ChatCommand) -> LaunchSpec {
    assert_eq!(f.command(tab, cmd), ok_result());
    let id = wait_for("the kickoff", || f.bridge.chat_session_id());
    f.launched(&id)
}

/// The opening delivered and its turn running, so the agent's blocks are read.
pub(super) fn working(f: &Fixture, s: &LaunchSpec) {
    opening(f, s);
    f.spool(s, "UserPromptSubmit", json!({"prompt": "opening"}));
    f.wait_state(s, SessionState::Working);
}

pub(super) fn idle(f: &Fixture, s: &LaunchSpec) {
    f.spool(s, "Stop", json!({}));
    f.wait_state(s, SessionState::Idle);
}

pub(super) fn end_now(f: &Fixture, s: &LaunchSpec) {
    f.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the ended session", || !f.ended(&s.id).is_empty());
}

#[test]
fn a_started_kickoff_writes_its_draft() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    store_file(&f);
    let typed = "i would like to add support for microvm test environment";
    let cmd = start_cmd("kickoff", Some(typed), vec![file_input(ID)]);
    let s = start_kickoff(&f, &tab, cmd);
    let id = s.id.clone();

    let draft = stored(&f, &id);
    assert_eq!(draft["session"], id.as_str());
    assert_eq!(draft["messages"], json!([typed]));
    let input = json!({"kind": "file", "id": ID, "name": "client-name.png"});
    assert_eq!(draft["attachments"], json!([input]));
    assert_eq!(draft["live"], true);
    for absent in ["shape", "name_override", "target"] {
        assert_eq!(draft[absent], Value::Null, "{absent}");
    }
    let sidecar = read_sidecar(&f.ctx.runtime().attachments, ID).unwrap();
    assert!(sidecar.stored_at > 1, "the draft keeps its file alive");

    working(&f, &s);
    let shape = json!({"size": "feature", "name": "agent-name", "problem": "P1"});
    block(&f, &s, &tab, shape);
    let draft = stored(&f, &id);
    assert_eq!(draft["shape"]["name"], "agent-name");
    assert_eq!(draft["shape"]["problem"], "P1");
    idle(&f, &s);

    assert_eq!(
        f.command(&tab, send("second thought", Vec::new())),
        ok_result()
    );
    assert_eq!(
        stored(&f, &id)["messages"],
        json!([typed, "second thought"])
    );

    assert_eq!(f.command(&tab, rename_cmd("renamed-work")), ok_result());
    let draft = stored(&f, &id);
    assert_eq!(draft["name_override"], "renamed-work");
    assert_eq!(draft["shape"]["name"], "agent-name", "the shape stays raw");
    assert_eq!(draft["messages"].as_array().unwrap().len(), 2);

    end_now(&f, &s);
    wait_until("the ended draft", || stored(&f, &id)["live"] == false);

    assert_read_back_after_restart(&f, &id, typed);
}

/// What a freshly started server reads from the files alone.
fn assert_read_back_after_restart(f: &Fixture, id: &str, typed: &str) {
    let fresh = ChatState::default();
    let listed = drafts::list(&f.ctx, &fresh);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name.as_deref(), Some("renamed-work"));
    assert_eq!(listed[0].size, Some(KickoffSize::Feature));
    assert_eq!(listed[0].problem.as_deref(), Some("P1"));
    assert!(!listed[0].live);
    let kept = drafts::get(&f.ctx, &fresh, id).unwrap();
    assert_eq!(kept.messages, vec![typed, "second thought"]);
    let shape: KickoffShape = kept.effective_shape().unwrap();
    assert_eq!(shape.name.as_deref(), Some("renamed-work"));
}

#[test]
fn a_rename_wins_over_a_later_agent_block() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = start_kickoff(&f, &tab, start_cmd("kickoff", None, Vec::new()));
    working(&f, &s);
    block(&f, &s, &tab, json!({"name": "agent-one", "problem": "A"}));
    assert_eq!(f.command(&tab, rename_cmd("mine")), ok_result());

    let later = json!({"name": "agent-two", "problem": "B"});
    let frame = block(&f, &s, &tab, later);

    assert_eq!(frame["shape"]["name"], "mine", "the broadcast view");
    let draft = stored(&f, &s.id);
    assert_eq!(draft["name_override"], "mine");
    assert_eq!(
        draft["shape"]["name"], "agent-two",
        "raw blocks stay comparable"
    );
    let listed = drafts::list(&f.ctx, &ChatState::default());
    assert_eq!(listed[0].name.as_deref(), Some("mine"));
    let reopened = f.connect(&browser()).opened;
    let kickoff = reopened.iter().find(|frame| frame["type"] == "kickoff");
    assert_eq!(kickoff.unwrap()["shape"]["name"], "mine", "a new tab");
}

#[test]
fn a_landed_kickoff_leaves_the_drafts() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = start_kickoff(&f, &tab, start_cmd("kickoff", Some("idea"), Vec::new()));
    working(&f, &s);
    block(
        &f,
        &s,
        &tab,
        json!({"size": "feature", "name": "fresh", "problem": "P"}),
    );
    idle(&f, &s);
    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "fresh")),
        ok_result()
    );
    assert_eq!(stored(&f, &s.id)["target"], "doc/product/fresh/index.md");
    assert_eq!(drafts::list(&f.ctx, &ChatState::default()).len(), 1);

    let document = f.ctx.root().join("doc/product/fresh/index.md");
    fs::create_dir_all(document.parent().unwrap()).unwrap();
    fs::write(&document, "# Fresh\n").unwrap();

    assert!(drafts::list(&f.ctx, &ChatState::default()).is_empty());
    assert!(
        !draft_file(&f, &s.id).exists(),
        "the landed draft is deleted"
    );
    end_now(&f, &s);
    assert!(
        !draft_file(&f, &s.id).exists(),
        "the end does not bring it back"
    );
}

#[test]
fn every_spelling_of_a_landed_plan_counts() {
    let f = fixture(false);
    let plans = f.ctx.root().join("doc/plans");
    fs::create_dir_all(plans.join("archive")).unwrap();
    let cases = [
        (
            "22222222-2222-4222-8222-222222222222",
            "IN_PROGRESS-PLAN-one.md",
            "",
        ),
        (
            "33333333-3333-4333-8333-333333333333",
            "DONE-PLAN-two.md",
            "",
        ),
        (
            "44444444-4444-4444-8444-444444444444",
            "RETIRED-PLAN-three.md",
            "",
        ),
        (
            "55555555-5555-4555-8555-555555555555",
            "PLAN-four.md",
            "archive/",
        ),
    ];
    for (session, file, dir) in cases {
        let slug = file.split("PLAN-").nth(1).unwrap().trim_end_matches(".md");
        fs::write(plans.join(dir).join(file), "# Plan\n").unwrap();
        let target = format!("doc/plans/PLAN-{slug}.md");
        write_draft(&f, session, json!({"target": target}));
    }
    write_draft(&f, OLD, json!({"target": "doc/plans/PLAN-absent.md"}));

    let listed = drafts::list(&f.ctx, &ChatState::default());

    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].session, OLD);
    assert!(!draft_file(&f, "22222222-2222-4222-8222-222222222222").exists());
}

/// A small kickoff with a file attached, shaped, renamed and set aside; returns its session.
fn set_aside_small_kickoff(f: &Fixture, tab: &Tab) -> LaunchSpec {
    store_file(f);
    let cmd = start_cmd("kickoff-small", None, vec![file_input(ID)]);
    let s = start_kickoff(f, tab, cmd);
    working(f, &s);
    let shape = json!({"size": "plan", "name": "agent-plan", "problem": "P", "decisions": ["D1"]});
    block(f, &s, tab, shape);
    idle(f, &s);
    assert_eq!(f.command(tab, rename_cmd("my-plan")), ok_result());
    assert_eq!(f.command(tab, set_aside_cmd(&s.id)), ok_result());
    f.finish_final_write(&s);
    wait_until("the ended session", || !f.ended(&s.id).is_empty());
    wait_until("the ended draft", || stored(f, &s.id)["live"] == false);
    s
}

/// The ended draft reads back from its file alone, with its name and size.
fn assert_ended_draft_listed(f: &Fixture) {
    let fresh = drafts::list(&f.ctx, &ChatState::default());
    assert_eq!(fresh.len(), 1, "the draft reads back from its file alone");
    assert_eq!(fresh[0].name.as_deref(), Some("my-plan"));
    assert_eq!(fresh[0].size, Some(KickoffSize::Plan));
    assert!(!fresh[0].live);
}

#[test]
fn a_continued_draft_carries_its_name_size_and_attachments() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = set_aside_small_kickoff(&f, &tab);
    let old_file = draft_file(&f, &s.id);
    assert!(old_file.exists(), "set aside keeps the draft");
    assert_ended_draft_listed(&f);
    let calls = record_guidance(&f);

    assert_eq!(f.command(&tab, continue_cmd(&s.id)), ok_result());

    let id = wait_for("the continued kickoff", || f.bridge.chat_session_id());
    assert_ne!(id, s.id);
    let seen = calls.calls.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0.as_deref(), Some("kickoff-small"), "small: true");
    let first = seen[0].1.clone().unwrap();
    assert!(first.contains("The work is named `my-plan`."), "{first}");
    assert!(first.contains("> D1"), "{first}");
    assert!(first.contains(ID), "the file attachment travels: {first}");
    let draft = stored(&f, &id);
    assert_eq!(draft["name_override"], "my-plan");
    assert_eq!(draft["shape"]["size"], "plan");
    assert_eq!(draft["shape"]["name"], "agent-plan");
    assert_eq!(draft["attachments"][0]["id"], ID);
    assert_eq!(draft["live"], true);
    assert!(
        !old_file.exists(),
        "the old draft goes once the new one stands"
    );
    let opened = f.connect(&browser()).opened;
    let kickoff = opened
        .iter()
        .find(|frame| frame["type"] == "kickoff")
        .unwrap();
    assert_eq!(kickoff["session"], id.as_str());
    assert_eq!(
        kickoff["shape"]["name"], "my-plan",
        "Taking shape starts filled"
    );
}
