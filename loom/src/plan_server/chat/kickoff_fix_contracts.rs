//! Contract tests for the kickoff fixes: the size is the offer's, set aside ends at once or
//! within a bound, a draft lands only when its session finished, and delete is confined to
//! the grant's files. They drive the real dispatch through the shared chat harness.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::tests::{browser, ok_result, wait_for, wait_until, Fixture, Tab};
use super::{drafts, ChatState};
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::model::SessionState;
use crate::plan_session::scope::{self, ArmRefusal, KickoffGrant, KickoffSize, SessionScope};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const PLAN: &str = "doc/plans/PLAN-demo-work.md";

fn cmd_start(offer: &str) -> ChatCommand {
    ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: Some(offer.into()),
        feature: None,
        plan: None,
        text: None,
        attachments: Vec::new(),
    }
}

fn set_aside(session: &str, now: bool) -> ChatCommand {
    ChatCommand::SetAsideKickoff {
        client_id: Some("c1".into()),
        session: session.into(),
        now,
    }
}

fn discard(session: &str, delete_files: bool) -> ChatCommand {
    ChatCommand::DiscardKickoff {
        client_id: Some("c1".into()),
        session: session.into(),
        delete_files,
    }
}

fn confirm(size: KickoffSize, name: &str) -> ChatCommand {
    ChatCommand::ConfirmKickoff {
        client_id: Some("c1".into()),
        size,
        name: name.into(),
    }
}

fn continue_cmd(session: &str) -> ChatCommand {
    ChatCommand::ContinueKickoff {
        client_id: Some("c1".into()),
        session: session.into(),
    }
}

fn refused(error: impl ToString) -> Value {
    json!({"type": "chat", "frame": "result", "client_id": "c1",
        "ok": false, "error": error.to_string()})
}

/// A kickoff started from `offer`; the session waits for its opening.
fn launch(f: &Fixture, tab: &Tab, offer: &str) -> LaunchSpec {
    assert_eq!(f.command(tab, cmd_start(offer)), ok_result());
    let id = wait_for("the chat", || f.bridge.chat_session_id());
    f.launched(&id)
}

/// The opening delivered and its turn running, so the agent's blocks are read.
fn working(f: &Fixture, s: &LaunchSpec) {
    f.spool(s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening paste", || {
        !f.host.delivered_to(&s.socket).is_empty()
    });
    f.spool(s, "UserPromptSubmit", json!({"prompt": "opening"}));
    f.wait_state(s, SessionState::Working);
}

fn idle(f: &Fixture, s: &LaunchSpec) {
    f.spool(s, "Stop", json!({}));
    f.wait_state(s, SessionState::Idle);
}

/// Append an assistant `loom-kickoff` block holding `shape`; the kickoff frame it produces.
fn block(f: &Fixture, s: &LaunchSpec, tab: &Tab, shape: Value) -> Value {
    let markdown = format!("```loom-kickoff\n{shape}\n```");
    let message = json!({"id": "block", "role": "assistant", "stop_reason": "end_turn",
        "content": [{"type": "text", "text": markdown}]});
    let record = json!({"type": "assistant", "message": message,
        "uuid": uuid::Uuid::new_v4().to_string(), "sessionId": s.id,
        "timestamp": chrono::Utc::now().to_rfc3339()});
    let path = f.bridge.transcripts(&s.id).unwrap().main.unwrap();
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    writeln!(file, "{record}").unwrap();
    wait_for("the kickoff frame", || {
        tab.frames().into_iter().rev().find(|frame| {
            frame["type"] == "kickoff" && frame["shape"]["problem"] == shape["problem"]
        })
    })
}

/// A launched kickoff of `offer` with a confirmed `size` and name `demo-work`, idle.
fn confirmed(f: &Fixture, tab: &Tab, offer: &str, size: KickoffSize) -> LaunchSpec {
    let s = launch(f, tab, offer);
    working(f, &s);
    block(f, &s, tab, json!({"name": "demo-work", "problem": "P"}));
    idle(f, &s);
    assert_eq!(f.command(tab, confirm(size, "demo-work")), ok_result());
    s
}

fn draft_file(f: &Fixture, session: &str) -> PathBuf {
    let dir = f.ctx.runtime().dir.join("kickoffs");
    dir.join(format!("{session}.json"))
}

fn stored(f: &Fixture, session: &str) -> Value {
    let text = fs::read_to_string(draft_file(f, session)).expect("a draft file");
    serde_json::from_str(&text).expect("a draft that parses")
}

/// An ended plan draft as a server run leaves it; `patch` replaces top-level keys.
fn write_draft(f: &Fixture, session: &str, patch: Value) -> PathBuf {
    let shape = json!({"size": "plan", "name": "demo-work", "proposal": false, "problem": "P"});
    let mut draft = json!({"session": session, "size": "plan", "finished": false,
        "started_at": "2026-11-04T10:47:11Z", "updated_at": "2026-11-04T10:50:00Z",
        "messages": ["first idea"], "shape": shape, "attachments": [],
        "target": PLAN, "live": false});
    for (key, value) in patch.as_object().into_iter().flatten() {
        draft[key] = value.clone();
    }
    let path = draft_file(f, session);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&draft).unwrap()).unwrap();
    path
}

fn touch(f: &Fixture, relative: &str) -> PathBuf {
    let path = f.ctx.root().join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "# file\n").unwrap();
    path
}

/// The harness has no clock to advance: its closing-write bound is 5 s, so a wait of 2 s
/// separates an end that happens at once from one that waits for the closing reply.
fn within_two_seconds(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !done() {
        assert!(Instant::now() < deadline, "{what} took over 2 s");
        thread::sleep(Duration::from_millis(10));
    }
}

fn kickoff_scope(s: &LaunchSpec) -> scope::KickoffScope {
    match scope::load(&s.dir).unwrap() {
        SessionScope::Kickoff(kickoff) => kickoff,
        other => panic!("kickoff scope missing: {other:?}"),
    }
}

/// Contract `feature-kickoff-keeps-feature-size`.
#[test]
fn feature_kickoff_keeps_feature_size() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "kickoff");
    working(&f, &s);

    let shape = json!({"size": "plan", "name": "demo-work", "problem": "A fresh feature"});
    let view = block(&f, &s, &tab, shape);

    assert_eq!(view["shape"]["size"], "feature", "the published view");
    assert_eq!(stored(&f, &s.id)["size"], "feature");
    let state = f.ctx.extensions().get::<ChatState>().unwrap();
    let listed = drafts::list(&f.ctx, &state);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].size, KickoffSize::Feature, "the draft listing");
}

/// Contract `confirm-refuses-other-size`.
#[test]
fn confirm_refuses_other_size() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "kickoff");

    let result = f.command(&tab, confirm(KickoffSize::Plan, "demo-work"));

    assert_eq!(result, refused(ArmRefusal::WrongSize(KickoffSize::Feature)));
    assert_eq!(kickoff_scope(&s).grant, None);
    assert!(f.host.delivered_to(&s.socket).is_empty());
    assert!(!f.ctx.root().join(PLAN).exists());
}

/// Contract `set-aside-unconfirmed-ends-at-once`.
#[test]
fn set_aside_unconfirmed_ends_at_once() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "kickoff");
    working(&f, &s);
    let message = ChatCommand::Send {
        client_id: Some("c1".into()),
        text: "an idea".into(),
        attachments: Vec::new(),
    };
    assert_eq!(f.command(&tab, message), ok_result());

    assert_eq!(f.command(&tab, set_aside(&s.id, false)), ok_result());

    within_two_seconds("the end of an unconfirmed set aside", || {
        !f.ended(&s.id).is_empty()
    });
    within_two_seconds("the draft going not live", || {
        stored(&f, &s.id)["live"] == false
    });
}

/// Contract `stop-now-tears-down-a-closing-write`.
#[test]
fn stop_now_tears_down_a_closing_write() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let s = confirmed(&f, &tab, "kickoff", KickoffSize::Feature);
    wait_until("the confirmation paste", || {
        f.host.delivered_to(&s.socket).len() == 2
    });
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "confirmed"}));
    f.wait_state(&s, SessionState::Working);

    assert_eq!(f.command(&tab, set_aside(&s.id, false)), ok_result());
    assert!(f.ended(&s.id).is_empty(), "the closing write is waited for");
    let second = f.command(&tab, set_aside(&s.id, true));

    assert_eq!(second, ok_result());
    within_two_seconds("the teardown", || !f.ended(&s.id).is_empty());
    within_two_seconds("the draft going not live", || {
        stored(&f, &s.id)["live"] == false
    });
}

/// Contract `set-aside-draft-with-document-stays-listed`.
#[test]
fn set_aside_draft_with_document_stays_listed() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let s = confirmed(&f, &tab, "kickoff-small", KickoffSize::Plan);
    touch(&f, PLAN);

    assert_eq!(f.command(&tab, set_aside(&s.id, true)), ok_result());
    within_two_seconds("the end", || !f.ended(&s.id).is_empty());
    within_two_seconds("the ended draft", || stored(&f, &s.id)["live"] == false);

    let listed = drafts::list(&f.ctx, &ChatState::default());
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].session, s.id);
    assert!(!listed[0].live);
    assert!(draft_file(&f, &s.id).exists());
}

/// Contract `continue-resumes-grant-for-existing-target`.
#[test]
fn continue_resumes_grant_for_existing_target() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    write_draft(&f, SESSION, json!({}));
    let document = touch(&f, PLAN);

    assert_eq!(f.command(&tab, continue_cmd(SESSION)), ok_result());

    let id = wait_for("the continuation", || f.bridge.chat_session_id());
    let kickoff = kickoff_scope(&f.launched(&id));
    let grant = KickoffGrant {
        size: KickoffSize::Plan,
        name: "demo-work".into(),
    };
    assert_eq!(kickoff.grant, Some(grant));
    assert_eq!(kickoff.size, KickoffSize::Plan);
    assert!(document.exists());
}

/// Contract `delete-removes-only-grant-files`.
#[test]
fn delete_removes_only_grant_files() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    write_draft(&f, SESSION, json!({}));
    let plan = touch(&f, PLAN);
    let bundle = touch(&f, "doc/plans/briefs/demo-work/a.md");
    let other_plan = touch(&f, "doc/plans/PLAN-other.md");
    let other_bundle = touch(&f, "doc/plans/briefs/other/b.md");

    assert_eq!(f.command(&tab, discard(SESSION, true)), ok_result());

    assert!(!plan.exists(), "the plan document");
    assert!(
        !bundle.exists() && !bundle.parent().unwrap().exists(),
        "the bundle"
    );
    assert!(other_plan.exists(), "a sibling plan");
    assert!(other_bundle.exists(), "a sibling bundle");
    assert!(!draft_file(&f, SESSION).exists(), "the draft");
}

/// Contract `delete-refuses-non-grant-target`.
#[test]
fn delete_refuses_non_grant_target() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let target = "doc/loom/knowledge/INDEX.md";
    write_draft(&f, SESSION, json!({"target": target}));
    let index = touch(&f, target);

    let result = f.command(&tab, discard(SESSION, true));

    assert_eq!(result["ok"], false, "{result}");
    assert!(index.exists(), "a file outside the grant is never removed");
    assert!(
        draft_file(&f, SESSION).exists(),
        "a refused delete keeps the draft"
    );
}

/// Contract `delete-does-not-follow-symlinked-bundle`.
#[cfg(unix)]
#[test]
fn delete_does_not_follow_symlinked_bundle() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    write_draft(&f, SESSION, json!({}));
    touch(&f, PLAN);
    let outside = tempfile::tempdir().unwrap();
    let keep = outside.path().join("keep.md");
    fs::write(&keep, "# keep\n").unwrap();
    let briefs = f.ctx.root().join("doc/plans/briefs");
    fs::create_dir_all(&briefs).unwrap();
    std::os::unix::fs::symlink(outside.path(), briefs.join("demo-work")).unwrap();

    f.command(&tab, discard(SESSION, true));

    assert!(
        keep.exists(),
        "the delete followed the symlink out of the project"
    );
}
