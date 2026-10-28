//! Tests of chat command dispatch: refusals, start forms and queue return.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::guidance::{FixedGuidance, GuidanceProvider, GuidanceSlot};
use super::tests::{browser, ok_result, start, wait_for, wait_until, Fixture, Tab};
use crate::plan_server::authoring::Refusal;
use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::chat::{ChatCommand, ChatFrame, CommandResult};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::attachments::{AttachmentError, AttachmentInput, AttachmentKind};
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::input::InputRefusal;
use crate::plan_session::kind::SessionDir;
use crate::plan_session::model::{Item, Origin, SessionState};

mod start;

fn attachment(path: &str) -> AttachmentInput {
    AttachmentInput {
        kind: AttachmentKind::Source,
        path: path.into(),
        anchor: Some("demo".into()),
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: "# Demo".into(),
        selected_hash: None,
    }
}

fn send(text: &str, attachments: Vec<AttachmentInput>) -> ChatCommand {
    ChatCommand::Send {
        client_id: Some("c1".into()),
        text: text.into(),
        attachments,
    }
}

fn refused(error: impl ToString) -> Value {
    json!({"type": "chat", "frame": "result", "client_id": "c1",
        "ok": false, "error": error.to_string()})
}

type GuidanceCall = (Option<String>, Option<String>);

#[derive(Default)]
struct RecordingGuidance {
    calls: Mutex<Vec<GuidanceCall>>,
}

impl GuidanceProvider for RecordingGuidance {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
    ) -> anyhow::Result<LaunchGuidance> {
        self.calls
            .lock()
            .unwrap()
            .push((offer.map(str::to_owned), first_message.map(str::to_owned)));
        FixedGuidance.authoring(ctx, dir, offer, first_message)
    }

    fn closing_message(&self) -> String {
        FixedGuidance.closing_message()
    }
}

fn record_guidance(f: &Fixture) -> Arc<RecordingGuidance> {
    let provider = Arc::new(RecordingGuidance::default());
    f.ctx
        .extensions()
        .get::<GuidanceSlot>()
        .unwrap()
        .set(provider.clone());
    provider
}

fn opening(f: &Fixture, s: &LaunchSpec) -> Item {
    f.spool(s, "SessionStart", json!({"source": "startup"}));
    let item = wait_for("the opening user item", || {
        f.bridge
            .chat_snapshot(true)
            .turns
            .into_iter()
            .flat_map(|turn| turn.items)
            .find(|item| matches!(item, Item::User { .. }))
    });
    wait_until("the opening paste", || {
        !f.host.delivered_to(&s.socket).is_empty()
    });
    item
}

/// The marker follows every frame already sent to this connection, including a stray return.
fn received_frames(f: &Fixture, tab: &Tab) -> Vec<Value> {
    let marker = CommandResult {
        client_id: Some("barrier".into()),
        ok: true,
        error: None,
    };
    f.ctx.hub().send_to_conn(
        tab.conn.id,
        &PlanMessage::Chat(Box::new(ChatFrame::Result(marker))),
    );
    wait_until("the connection barrier", || {
        tab.frames()
            .iter()
            .any(|frame| frame["client_id"] == "barrier")
    });
    tab.frames()
}

#[test]
fn observer_commands_are_refused_while_another_browser_holds_authoring() {
    let f = Fixture::new(false);
    let holder = f.connect(&browser());
    let observer = f.connect(&browser());
    let s = f.start(&holder);
    let error = f.ctx.refusal_message(&Refusal::NotHolder {
        holder: holder.conn.public_id.clone(),
    });
    let commands = [
        send("observer message", Vec::new()),
        ChatCommand::Permission {
            client_id: Some("c1".into()),
            request_id: "unknown".into(),
            allow: true,
            note: None,
        },
        ChatCommand::Interrupt {
            client_id: Some("c1".into()),
        },
        ChatCommand::End {
            client_id: Some("c1".into()),
        },
    ];

    for command in commands {
        assert_eq!(f.command(&observer, command), refused(&error));
    }
    assert!(f.ctx.authoring().is_holder(&holder.conn.browser));
    assert_eq!(f.bridge.chat_session_id(), Some(s.id));
    assert!(f.bridge.chat_snapshot(true).queue.is_empty());
    assert!(!received_frames(&f, &holder)
        .iter()
        .any(|frame| { frame["frame"] == "result" && frame["ok"] == false }));
}

#[test]
fn a_slash_command_is_refused_with_the_input_refusal_text() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());

    let result = f.command(&tab, send("\0  /help", Vec::new()));

    assert_eq!(result, refused(InputRefusal::SlashCommand));
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn a_message_over_100_kb_is_refused_with_the_input_refusal_text() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());

    let result = f.command(&tab, send(&"x".repeat(100_001), Vec::new()));

    assert_eq!(
        result,
        refused(InputRefusal::TooLong {
            bytes: 100_001,
            limit: 100_000,
        })
    );
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn eleven_attachments_are_refused() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let inputs = vec![attachment("doc/product/index.md"); 11];

    let result = f.command(&tab, send("review", inputs));

    assert_eq!(result, refused(InputRefusal::TooManyAttachments));
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn an_attachment_with_a_parent_path_is_refused() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let path = "doc/product/../product/index.md";

    let result = f.command(&tab, send("review", vec![attachment(path)]));

    assert_eq!(result, refused(AttachmentError::Path(path.into())));
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn start_is_refused_in_read_only_mode() {
    let f = Fixture::new(true);
    let tab = f.connect(&browser());

    let result = f.command(&tab, start());

    assert_eq!(result, refused("loom plan is in read-only mode"));
    assert!(f.ctx.authoring().holder().is_none());
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn start_is_refused_while_the_server_is_stopping() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    f.ctx.shutdown().abort_all();

    let result = f.command(&tab, start());

    assert_eq!(result, refused("loom plan is stopping"));
    assert!(f.ctx.authoring().holder().is_none());
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn start_without_offer_or_text_takes_authoring_and_marks_the_opening_as_start() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let guidance = record_guidance(&f);
    assert!(f.ctx.authoring().holder().is_none());

    let s = f.start(&tab);
    let item = opening(&f, &s);

    assert!(f.ctx.authoring().is_holder(&tab.conn.browser));
    assert_eq!(*guidance.calls.lock().unwrap(), vec![(None, None)]);
    assert!(matches!(
        item,
        Item::User {
            origin: Some(Origin::Start),
            ..
        }
    ));
}

#[test]
fn start_with_a_current_offer_marks_the_opening_as_offer() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let guidance = record_guidance(&f);
    let offer = {
        let project = f.ctx.project();
        project
            .checklist
            .primary
            .iter()
            .chain(&project.checklist.secondary)
            .next()
            .expect("the fixture has an offered step")
            .id()
    };
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: Some(offer.clone()),
        text: None,
        attachments: Vec::new(),
    };

    assert_eq!(f.command(&tab, cmd), ok_result());
    let id = wait_for("the offered chat", || f.bridge.chat_session_id());
    let item = opening(&f, &f.launched(&id));

    assert_eq!(
        *guidance.calls.lock().unwrap(),
        vec![(Some(offer.clone()), None)]
    );
    assert!(matches!(
        item,
        Item::User {
            origin: Some(Origin::Offer),
            ..
        }
    ));
    assert_eq!(
        f.bridge.chat_snapshot(true).session.unwrap().offer,
        Some(offer)
    );
}

#[test]
fn a_typed_start_passes_the_formatted_first_message_and_has_no_origin() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let guidance = record_guidance(&f);
    let expected = "Review\nthis\n\n<loom-attachments>\n\
        [1] source doc/product/index.md#demo\n> # Demo\n</loom-attachments>";
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        text: Some("\0Review\r\nthis".into()),
        attachments: vec![attachment("doc/product/index.md")],
    };

    assert_eq!(f.command(&tab, cmd), ok_result());
    let id = wait_for("the typed chat", || f.bridge.chat_session_id());
    let s = f.launched(&id);
    let item = opening(&f, &s);

    assert_eq!(
        *guidance.calls.lock().unwrap(),
        vec![(None, Some(expected.into()))]
    );
    assert_eq!(f.host.delivered_to(&s.socket), vec![expected.to_owned()]);
    assert!(matches!(item, Item::User { origin: None, text, .. }
        if text == "Review\nthis"));
}

#[test]
fn queue_returned_reaches_only_the_interrupting_connection() {
    let f = Fixture::new(false);
    let holder = browser();
    let tab = f.connect(&holder);
    let second = f.connect(&holder);
    let observer = f.connect(&browser());
    let s = f.idle_chat(&tab);
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "active turn"}));
    f.wait_state(&s, SessionState::Working);
    assert_eq!(
        f.command(&tab, send("first queued", Vec::new())),
        ok_result()
    );
    assert_eq!(
        f.command(&second, send("second queued", Vec::new())),
        ok_result()
    );

    let stop = ChatCommand::Interrupt {
        client_id: Some("c1".into()),
    };
    assert_eq!(f.command(&second, stop), ok_result());
    let returned = wait_for("the returned queue", || {
        second
            .frames()
            .into_iter()
            .find(|frame| frame["frame"] == "queue_returned")
    });

    assert_eq!(
        returned,
        json!({"type": "chat", "frame": "queue_returned",
        "texts": ["first queued", "second queued"]})
    );
    for other in [&tab, &observer] {
        let frames = received_frames(&f, other);
        assert!(
            !frames
                .iter()
                .any(|frame| frame["frame"] == "queue_returned"),
            "{frames:?}"
        );
    }
    assert!(f.bridge.chat_snapshot(true).queue.is_empty());
}
