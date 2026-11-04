//! The four draft commands: what each accepts and what each refuses.

use std::fs;

use serde_json::json;

use super::super::drafts;
use super::super::tests::{browser, ok_result, wait_for};
use super::super::ChatState;
use super::drafts::{
    continue_cmd, discard_cmd, draft_file, idle, rename_cmd, set_aside_cmd, start_cmd,
    start_kickoff, stored, working, write_draft, OLD,
};
use super::kickoff::{block, confirm};
use super::{fixture, record_guidance, refused};
use crate::plan_server::authoring::Refusal;
use crate::plan_session::scope::{ArmRefusal, KickoffSize};

const UNKNOWN: &str = "There is no such unfinished kickoff";

#[test]
fn rename_refuses_a_bad_name_a_missing_shape_and_a_confirmed_kickoff() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = start_kickoff(&f, &tab, start_cmd("kickoff", None, Vec::new()));
    working(&f, &s);

    assert_eq!(
        f.command(&tab, rename_cmd("mine")),
        refused("Nothing to rename yet")
    );
    block(
        &f,
        &s,
        &tab,
        json!({"size": "feature", "name": "fresh", "problem": "P"}),
    );
    idle(&f, &s);
    let invalid = ArmRefusal::InvalidName("Bad Name".into());
    assert_eq!(f.command(&tab, rename_cmd("Bad Name")), refused(invalid));
    assert_eq!(f.command(&tab, rename_cmd("mine")), ok_result());
    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "mine")),
        ok_result()
    );

    assert_eq!(
        f.command(&tab, rename_cmd("other")),
        refused("The name is fixed once the work is confirmed")
    );
    assert_eq!(super::drafts::stored(&f, &s.id)["name_override"], "mine");
}

#[test]
fn rename_is_refused_for_a_browser_that_does_not_hold_authoring() {
    let f = fixture(false);
    let holder = f.connect(&browser());
    let observer = f.connect(&browser());
    let s = start_kickoff(&f, &holder, start_cmd("kickoff", None, Vec::new()));
    working(&f, &s);
    block(&f, &s, &holder, json!({"name": "fresh", "problem": "P"}));
    let error = f.ctx.refusal_message(&Refusal::NotHolder {
        holder: holder.conn.public_id.clone(),
    });

    assert_eq!(f.command(&observer, rename_cmd("mine")), refused(&error));
    assert_eq!(f.command(&observer, discard_cmd(&s.id)), refused(&error));
    assert_eq!(
        super::drafts::stored(&f, &s.id)["name_override"],
        json!(null)
    );
}

#[test]
fn set_aside_and_continue_refuse_an_unknown_or_malformed_session() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = start_kickoff(&f, &tab, start_cmd("kickoff", None, Vec::new()));
    let unknown = "99999999-9999-4999-8999-999999999999";

    for session in [
        unknown,
        "../../etc/passwd",
        "ABCDEF12-1111-4111-8111-111111111111",
    ] {
        let error = if session == unknown {
            UNKNOWN.to_owned()
        } else {
            format!("{session:?} is not a kickoff session id")
        };
        for command in [set_aside_cmd(session), continue_cmd(session)] {
            assert_eq!(f.command(&tab, command), refused(&error), "{session}");
        }
    }
    assert_eq!(f.bridge.chat_session_id(), Some(s.id));
}

#[test]
fn set_aside_refuses_a_draft_that_is_not_the_open_kickoff() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    start_kickoff(&f, &tab, start_cmd("kickoff", None, Vec::new()));
    write_draft(&f, OLD, json!({}));

    assert_eq!(
        f.command(&tab, set_aside_cmd(OLD)),
        refused("That kickoff is not open")
    );
    assert!(draft_file(&f, OLD).exists());
}

#[test]
fn continue_is_refused_while_another_chat_is_open_and_keeps_the_old_draft() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let old = write_draft(&f, OLD, json!({}));
    let before = fs::read(&old).unwrap();
    f.start(&tab);

    let result = f.command(&tab, continue_cmd(OLD));

    assert_eq!(result["ok"], false, "{result}");
    assert!(!result["error"].as_str().unwrap().is_empty());
    assert_eq!(
        fs::read(&old).unwrap(),
        before,
        "the old draft is untouched"
    );
    assert_eq!(fs::read_dir(old.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn a_full_draft_continues_within_the_delivery_limit() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let messages: Vec<String> = (0..40)
        .map(|n| format!("message {n:02} {}", "x".repeat(4000)))
        .collect();
    write_draft(&f, OLD, json!({"messages": messages}));
    let calls = record_guidance(&f);

    assert_eq!(f.command(&tab, continue_cmd(OLD)), ok_result());

    let first = calls.calls.lock().unwrap()[0].1.clone().unwrap();
    assert!(first.len() < 100_000, "{} bytes", first.len());
    assert!(first.contains("(earlier messages omitted)"));
    assert!(first.contains("message 00 ") && first.contains("message 39 "));
    assert!(!draft_file(&f, OLD).exists());
}

#[test]
fn an_unrenamed_draft_continues_without_pinning_the_agents_name() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    write_draft(&f, OLD, json!({}));

    assert_eq!(f.command(&tab, continue_cmd(OLD)), ok_result());

    let id = wait_for("the continuation", || f.bridge.chat_session_id());
    assert!(stored(&f, &id)["name_override"].is_null());
    let spec = f.launched(&id);
    working(&f, &spec);
    let later = json!({"size": "feature", "name": "agent-two", "problem": "B"});
    let frame = block(&f, &spec, &tab, later);
    assert_eq!(frame["shape"]["name"], "agent-two", "the broadcast view");
    assert!(stored(&f, &id)["name_override"].is_null());
}

#[test]
fn an_attachment_that_is_gone_is_named_not_fatal() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let gone = json!({"kind": "file", "id": "ffffffffffffffff", "name": "gone.png"});
    write_draft(&f, OLD, json!({"attachments": [gone]}));
    let calls = record_guidance(&f);

    assert_eq!(f.command(&tab, continue_cmd(OLD)), ok_result());

    let id = wait_for("the continuation", || f.bridge.chat_session_id());
    let first = calls.calls.lock().unwrap()[0].1.clone().unwrap();
    assert!(
        first.contains("(attachment no longer available: gone.png)"),
        "{first}"
    );
    assert_eq!(super::drafts::stored(&f, &id)["attachments"], json!([]));
    assert!(!draft_file(&f, OLD).exists());
}

#[test]
fn discard_deletes_an_ended_draft_and_refuses_a_live_one() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let ended = write_draft(&f, OLD, json!({}));
    let s = start_kickoff(&f, &tab, start_cmd("kickoff", None, Vec::new()));

    assert_eq!(
        f.command(&tab, discard_cmd(&s.id)),
        refused("Set it aside first")
    );
    assert!(draft_file(&f, &s.id).exists());
    assert_eq!(f.command(&tab, discard_cmd(OLD)), ok_result());

    assert!(!ended.exists());
    let listed = drafts::list(&f.ctx, &ChatState::default());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].session, s.id);
    assert!(listed[0].live);
    assert_eq!(f.command(&tab, discard_cmd(OLD)), refused(UNKNOWN));
    assert_eq!(
        f.command(&tab, discard_cmd("not-a-session")),
        refused("\"not-a-session\" is not a kickoff session id")
    );
}
