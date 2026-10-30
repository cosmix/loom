//! What a chat belongs to: the plan a start names, and what a send's fallback start keeps.

use super::super::tests::{browser, ok_result, wait_for, wait_until, Fixture, Tab};
use super::{fixture, refused, send};
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::bridge::{EndCause, EndMode};
use crate::planning::project::Project;

/// A chat started for `feature` or `plan`, ended at once, then a send with no chat running.
fn send_after_chat_with(f: &Fixture, tab: &Tab, feature: Option<&str>, plan: Option<&str>) {
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        feature: feature.map(str::to_owned),
        plan: plan.map(str::to_owned),
        text: None,
        attachments: Vec::new(),
    };
    assert_eq!(f.command(tab, cmd), ok_result());
    let first = wait_for("the chat", || f.bridge.chat_session_id());
    f.launched(&first);
    let ended = f.bridge.end(&first, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("the end", || f.bridge.chat_session_id().is_none());
    assert_eq!(f.command(tab, send("Carry on", Vec::new())), ok_result());
    wait_for("the replacement", || f.bridge.chat_session_id());
}

#[test]
fn a_send_after_a_chat_died_keeps_its_feature() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    send_after_chat_with(&f, &tab, Some("demo"), None);
    let session = f.bridge.chat_snapshot(true).session.unwrap();
    assert_eq!(session.feature.as_deref(), Some("demo"));
    assert_eq!(session.plan, None);
}

#[test]
fn a_send_after_a_chat_died_keeps_its_plan() {
    let f = with_standalone_plan();
    let tab = f.connect(&browser());
    send_after_chat_with(&f, &tab, None, Some("solo"));
    let session = f.bridge.chat_snapshot(true).session.unwrap();
    assert_eq!(session.plan.as_deref(), Some("solo"));
    assert_eq!(session.feature, None);
}

/// A kickoff names its feature before it writes it: the send must not be refused over that.
#[test]
fn a_send_after_a_chat_died_drops_an_owner_the_project_does_not_have() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let first = f.start(&tab);
    f.bridge
        .set_owner(&first.id, Some("not-written-yet".into()), None)
        .unwrap();
    let ended = f.bridge.end(&first.id, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("the end", || f.bridge.chat_session_id().is_none());
    assert_eq!(f.command(&tab, send("Carry on", Vec::new())), ok_result());
    wait_for("the replacement", || f.bridge.chat_session_id());
    let session = f.bridge.chat_snapshot(true).session.unwrap();
    assert_eq!(session.feature, None);
}

fn with_standalone_plan() -> Fixture {
    let f = fixture(false);
    let root = f.ctx.root();
    std::fs::create_dir_all(root.join("doc/plans")).unwrap();
    std::fs::write(
        root.join("doc/plans/PLAN-solo.md"),
        "# Solo\nImplements: none\n",
    )
    .unwrap();
    *f.ctx.project_mut() = Project::load(root).unwrap();
    f
}

fn start_for_plan(plan: &str) -> ChatCommand {
    ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        feature: None,
        plan: Some(plan.into()),
        text: None,
        attachments: Vec::new(),
    }
}

#[test]
fn a_chat_started_for_a_standalone_plan_belongs_to_it() {
    let f = with_standalone_plan();
    let tab = f.connect(&browser());
    assert_eq!(f.command(&tab, start_for_plan("solo")), ok_result());
    let session = f.bridge.chat_snapshot(true).session.unwrap();
    assert_eq!(session.plan.as_deref(), Some("solo"));
    assert_eq!(session.feature, None);
}

#[test]
fn a_start_for_an_unknown_plan_is_refused_and_leaves_authoring_free() {
    let f = with_standalone_plan();
    let tab = f.connect(&browser());
    assert_eq!(
        f.command(&tab, start_for_plan("../missing")),
        refused("unknown plan: ../missing")
    );
    assert!(f.ctx.authoring().holder().is_none());
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn a_start_naming_both_a_feature_and_a_plan_is_refused() {
    let f = with_standalone_plan();
    let tab = f.connect(&browser());
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        feature: Some("demo".into()),
        plan: Some("solo".into()),
        text: None,
        attachments: Vec::new(),
    };
    assert_eq!(
        f.command(&tab, cmd),
        refused("a chat belongs to a feature or a plan, not both")
    );
    assert!(f.bridge.chat_session_id().is_none());
}
