//! The draft commands: what each accepts and what each refuses.

use std::fs;

use serde_json::json;

use super::super::drafts;
use super::super::tests::{browser, ok_result, wait_for, wait_until, Fixture};
use super::super::ChatState;
use super::drafts::{
    continue_cmd, discard_cmd, discard_files_cmd, draft_file, end_now, idle, set_aside_cmd,
    start_cmd, start_kickoff, stored, working, write_draft, OLD,
};
use super::kickoff::{block, confirm, ready_proposal};
use super::{fixture, record_guidance, refused};
use crate::plan_server::authoring::Refusal;
use crate::plan_session::scope::{self, KickoffGrant, KickoffSize, SessionScope};

const UNKNOWN: &str = "There is no such unfinished kickoff";
const PLAN: &str = "doc/plans/PLAN-demo-work.md";

/// A plan draft whose confirmed document is `PLAN`; the document exists when `written`.
fn plan_draft(f: &Fixture, written: bool) -> std::path::PathBuf {
    write_draft(f, OLD, json!({"size": "plan", "target": PLAN}));
    let document = f.ctx.root().join(PLAN);
    if written {
        fs::create_dir_all(document.parent().unwrap()).unwrap();
        fs::write(&document, "# Plan\n").unwrap();
    }
    document
}

#[test]
fn discard_is_refused_for_a_browser_that_does_not_hold_authoring() {
    let f = fixture(false);
    let holder = f.connect(&browser());
    let observer = f.connect(&browser());
    let s = start_kickoff(&f, &holder, start_cmd("kickoff", None, Vec::new()));
    working(&f, &s);
    block(&f, &s, &holder, json!({"name": "fresh", "problem": "P"}));
    let error = f.ctx.refusal_message(&Refusal::NotHolder {
        holder: holder.conn.public_id.clone(),
    });

    assert_eq!(f.command(&observer, discard_cmd(&s.id)), refused(&error));
    assert!(draft_file(&f, &s.id).exists());
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
fn a_continued_draft_follows_the_agents_later_name() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    write_draft(&f, OLD, json!({}));

    assert_eq!(f.command(&tab, continue_cmd(OLD)), ok_result());

    let id = wait_for("the continuation", || f.bridge.chat_session_id());
    let spec = f.launched(&id);
    working(&f, &spec);
    let later = json!({"size": "feature", "name": "agent-two", "problem": "B"});
    let frame = block(&f, &spec, &tab, later);
    assert_eq!(frame["shape"]["name"], "agent-two", "the broadcast view");
    assert_eq!(stored(&f, &id)["shape"]["name"], "agent-two");
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

#[test]
fn discard_that_would_leave_written_files_behind_is_refused() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let document = plan_draft(&f, true);

    assert_eq!(
        f.command(&tab, discard_files_cmd(OLD, false)),
        refused("This draft wrote files; delete them too or keep the draft")
    );
    assert!(draft_file(&f, OLD).exists() && document.exists());

    assert_eq!(f.command(&tab, discard_files_cmd(OLD, true)), ok_result());
    assert!(!draft_file(&f, OLD).exists() && !document.exists());
}

#[test]
fn discard_of_a_draft_whose_files_are_gone_removes_only_the_draft() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    plan_draft(&f, false);

    assert_eq!(f.command(&tab, discard_files_cmd(OLD, false)), ok_result());

    assert!(!draft_file(&f, OLD).exists());
}

/// A feature draft (hand-edited) whose target is a plan document: its files are never deleted,
/// it does not continue under the plan's grant, and it can still go on its own.
#[test]
fn a_draft_whose_target_is_of_another_size_never_deletes_or_resumes_it() {
    const MISMATCH: &str = "This draft's target does not match its size";
    let f = fixture(false);
    let tab = f.connect(&browser());
    write_draft(&f, OLD, json!({"size": "feature", "target": PLAN}));
    let document = f.ctx.root().join(PLAN);
    fs::create_dir_all(document.parent().unwrap()).unwrap();
    fs::write(&document, "# Plan\n").unwrap();

    let deleting = f.command(&tab, discard_files_cmd(OLD, true));
    assert_eq!(deleting, refused(MISMATCH));
    assert!(document.exists() && draft_file(&f, OLD).exists());
    assert_eq!(f.command(&tab, continue_cmd(OLD)), refused(MISMATCH));
    assert!(
        f.bridge.chat_session_id().is_none(),
        "no continuation started"
    );

    assert_eq!(f.command(&tab, discard_files_cmd(OLD, false)), ok_result());
    assert!(document.exists() && !draft_file(&f, OLD).exists());
}

#[test]
fn an_ended_session_is_finished_only_when_the_agent_asked() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let s = start_kickoff(&f, &tab, start_cmd("kickoff", Some("idea"), Vec::new()));
    working(&f, &s);
    block(&f, &s, &tab, json!({"name": "fresh", "problem": "P"}));
    idle(&f, &s);
    assert_eq!(
        f.command(&tab, confirm(KickoffSize::Feature, "fresh")),
        ok_result()
    );
    let document = f.ctx.root().join("doc/product/fresh/index.md");
    fs::create_dir_all(document.parent().unwrap()).unwrap();
    fs::write(&document, "# Fresh\n").unwrap();

    end_now(&f, &s);

    wait_until("the ended draft", || stored(&f, &s.id)["live"] == false);
    assert_eq!(stored(&f, &s.id)["finished"], false);
    let listed = drafts::list(&f.ctx, &ChatState::default());
    assert_eq!(listed.len(), 1, "the unfinished draft stays: {listed:?}");
    assert!(!listed[0].live);
}

#[test]
fn continuing_a_draft_with_its_document_resumes_the_grant_and_keeps_the_target() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    plan_draft(&f, true);
    let calls = record_guidance(&f);

    assert_eq!(f.command(&tab, continue_cmd(OLD)), ok_result());

    let id = wait_for("the continuation", || f.bridge.chat_session_id());
    let first = calls.calls.lock().unwrap()[0].1.clone().unwrap();
    let said = format!("The confirmed document `{PLAN}` exists and may be unfinished.");
    assert!(first.contains(&said), "{first}");
    let SessionScope::Kickoff(kickoff) = scope::load(&f.launched(&id).dir).unwrap() else {
        panic!("a kickoff scope");
    };
    let grant = KickoffGrant {
        size: KickoffSize::Plan,
        name: "demo-work".into(),
    };
    assert_eq!(kickoff.grant, Some(grant));
    assert_eq!(stored(&f, &id)["target"], PLAN, "the new draft keeps it");
    let view = f.connect(&browser()).opened;
    let view = view.iter().find(|frame| frame["type"] == "kickoff");
    assert_eq!(
        view.unwrap()["target"],
        PLAN,
        "the Write button reads written"
    );
    assert!(!draft_file(&f, OLD).exists());
}

#[test]
fn continuing_a_draft_whose_document_is_gone_holds_no_grant() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    plan_draft(&f, false);
    let calls = record_guidance(&f);

    assert_eq!(f.command(&tab, continue_cmd(OLD)), ok_result());

    let id = wait_for("the continuation", || f.bridge.chat_session_id());
    let first = calls.calls.lock().unwrap()[0].1.clone().unwrap();
    assert!(!first.contains("The confirmed document"), "{first}");
    let SessionScope::Kickoff(kickoff) = scope::load(&f.launched(&id).dir).unwrap() else {
        panic!("a kickoff scope");
    };
    assert_eq!(kickoff.grant, None);
    assert_eq!(kickoff.size, KickoffSize::Plan);
}

#[test]
fn continuing_a_draft_with_only_its_bundle_resumes_the_grant_without_claiming_a_document() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    plan_draft(&f, false);
    let bundle = f.ctx.root().join("doc/plans/briefs/demo-work");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join("a.md"), "# Brief\n").unwrap();
    let calls = record_guidance(&f);

    assert_eq!(f.command(&tab, continue_cmd(OLD)), ok_result());

    let id = wait_for("the continuation", || f.bridge.chat_session_id());
    let first = calls.calls.lock().unwrap()[0].1.clone().unwrap();
    assert!(!first.contains("The confirmed document"), "{first}");
    let SessionScope::Kickoff(kickoff) = scope::load(&f.launched(&id).dir).unwrap() else {
        panic!("a kickoff scope");
    };
    let grant = KickoffGrant {
        size: KickoffSize::Plan,
        name: "demo-work".into(),
    };
    assert_eq!(
        kickoff.grant,
        Some(grant),
        "the bundle can still be written"
    );
}

#[test]
fn a_second_kickoff_cannot_confirm_a_name_another_draft_holds() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    write_draft(
        &f,
        OLD,
        json!({"size": "feature", "target": "doc/product/fresh/index.md"}),
    );
    let s = ready_proposal(&f, &tab, "kickoff");

    let refusal = f.command(&tab, confirm(KickoffSize::Feature, "fresh"));

    assert_eq!(
        refusal,
        refused(
            "Another unfinished kickoff already holds doc/product/fresh/index.md. Continue or delete it from the draft shelf first."
        )
    );
    let SessionScope::Kickoff(kickoff) = scope::load(&s.dir).unwrap() else {
        panic!("kickoff scope missing");
    };
    assert_eq!(kickoff.grant, None, "nothing was armed");
    assert!(draft_file(&f, OLD).exists());
}
