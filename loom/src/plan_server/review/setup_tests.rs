//! Tests for the review setup hooks: reconnect replay and open-question broadcasts.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::SystemTime;

use super::commands::dispatch;
use super::state::ReviewState;
use crate::plan_server::authoring::{AuthoringEvent, FreeReason, Take};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, ConnCtx};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::review::{
    ReviewCommand, ReviewMessage, SuggestedPassageWire, SuggestionStatus,
};
use crate::planning::watch::{Batch, OwnWrites};

const SECTION: &str = "doc/product/demo/section_01.md";

struct Fixture {
    _dir: tempfile::TempDir,
    ctx: Arc<PlanContext>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_early_subscriber(|_, _| {})
    }

    /// A fixture whose `early` authoring subscriber runs before review's own, as
    /// `broadcast_people` does in production: the authoring state has already changed when it
    /// runs, and review's holder-change drop follows it.
    fn with_early_subscriber(
        early: impl Fn(&Arc<PlanContext>, &AuthoringEvent) + Send + Sync + 'static,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("doc/product/demo")).unwrap();
        std::fs::write(dir.path().join("doc/product/demo/index.md"), "# Demo\n").unwrap();
        std::fs::write(dir.path().join(SECTION), "# Goals\n\n## Open questions\n").unwrap();
        let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
        let ctx = Arc::new(
            PlanContext::new(
                dir.path().to_path_buf(),
                ReadOnly::from_problems(vec![]),
                bind,
                OwnWrites::default(),
            )
            .unwrap(),
        );
        let weak = Arc::downgrade(&ctx);
        ctx.authoring().subscribe(Box::new(move |event| {
            if let Some(ctx) = weak.upgrade() {
                early(&ctx, event);
            }
        }));
        super::setup::install(&ctx);
        Self { _dir: dir, ctx }
    }

    fn connect(&self, name: &str) -> Tab {
        let browser = BrowserId::mint().unwrap();
        self.ctx.presence().set_name(&browser, name);
        self.tab(&browser)
    }

    fn tab(&self, browser: &BrowserId) -> Tab {
        let conn = self.ctx.conn_ctx(browser.clone());
        let rx = self.ctx.hub().register(&conn);
        self.ctx.authoring().connection_opened(browser);
        self.ctx.presence().opened(&conn);
        Tab { conn, rx }
    }

    fn edit_batch(&self, rel: &str, text: &str) {
        std::fs::write(self.ctx.root().join(rel), text).unwrap();
        let batch = Batch {
            changed: vec![rel.into()],
            ..Batch::default()
        };
        self.ctx.project_mut().refresh(&batch).unwrap();
        for hook in self.ctx.batch_hooks() {
            hook(&self.ctx, &batch);
        }
    }
}

struct Tab {
    conn: ConnCtx,
    rx: mpsc::Receiver<Arc<String>>,
}

impl Tab {
    fn raw(&self) -> Vec<String> {
        self.rx.try_iter().map(|frame| (*frame).clone()).collect()
    }

    fn frames(&self) -> Vec<PlanMessage> {
        parse(&self.raw())
    }
}

fn parse(raw: &[String]) -> Vec<PlanMessage> {
    raw.iter()
        .map(|frame| serde_json::from_str(frame).unwrap())
        .collect()
}

/// No frame holds a browser's cookie value; frames route by cookie but carry public ids only.
fn assert_no_cookies(raw: &[String], tabs: &[&Tab]) {
    for frame in raw {
        for tab in tabs {
            assert!(!frame.contains(tab.conn.browser.as_str()), "{frame}");
        }
    }
}

fn passage() -> SuggestedPassageWire {
    SuggestedPassageWire {
        path: SECTION.into(),
        anchor: "goals".into(),
        text: "Keep one author at a time.".into(),
        lines: Some((3, 4)),
        stage: Some("review".into()),
        summary_sentence: Some("One author".into()),
    }
}

fn open_frames(ctx: &PlanContext, conn: &ConnCtx) -> Vec<PlanMessage> {
    ctx.hub()
        .open_frames(conn)
        .iter()
        .map(|frame| serde_json::from_str(frame).unwrap())
        .collect()
}

#[test]
fn holder_reconnect_receives_pending() {
    let f = Fixture::new();
    let holder = f.connect("Dana");
    let sender = f.connect("Sam");
    f.ctx
        .require_authoring(&holder.conn.browser, Take::TakeIfFree)
        .unwrap();
    dispatch(
        &f.ctx,
        &sender.conn,
        ReviewCommand::Suggest {
            passage: passage(),
            note: "Please review".into(),
        },
    );
    f.ctx.hub().unregister(holder.conn.id);
    f.ctx.presence().closed(holder.conn.id);
    f.ctx
        .authoring()
        .connection_closed(&holder.conn.browser, SystemTime::UNIX_EPOCH);

    let returning = f.tab(&holder.conn.browser);
    let viewer = f.tab(&sender.conn.browser);

    assert_ne!(returning.conn.id, holder.conn.id);
    assert_eq!(
        open_frames(&f.ctx, &returning.conn),
        vec![PlanMessage::Review(ReviewMessage::Suggestion {
            id: "sg-1".into(),
            from: "Sam".into(),
            passage: passage(),
            note: "Please review".into(),
        })],
    );
    assert!(open_frames(&f.ctx, &viewer.conn).is_empty());
}

#[test]
fn open_questions_broadcast_once_after_a_batch() {
    let f = Fixture::new();
    let first = f.connect("Dana");
    let second = f.connect("Sam");
    assert!(first.frames().is_empty());
    assert!(second.frames().is_empty());

    f.edit_batch(
        SECTION,
        "# Goals\n\n## Open questions\n- Who reviews the draft?\n",
    );

    let expected = vec![PlanMessage::Review(ReviewMessage::OpenQuestions {
        counts: BTreeMap::from([(SECTION.to_owned(), 1)]),
    })];
    assert_eq!(first.frames(), expected);
    assert_eq!(second.frames(), expected);

    f.edit_batch("doc/product/demo/index.md", "# Demo\n\nUpdated overview.\n");

    assert!(first.frames().is_empty());
    assert!(second.frames().is_empty());
}

fn suggest_command() -> ReviewCommand {
    ReviewCommand::Suggest {
        passage: passage(),
        note: "Please review".into(),
    }
}

fn dropped(id: &str) -> PlanMessage {
    PlanMessage::Review(ReviewMessage::SuggestionStatus {
        id: id.into(),
        status: SuggestionStatus::Dropped {
            reason: "Authoring holder changed.".into(),
        },
    })
}

fn assert_holder_sees_card_then_drop(to_holder: Vec<String>) {
    assert_eq!(
        parse(&to_holder),
        vec![
            PlanMessage::Review(ReviewMessage::Suggestion {
                id: "sg-1".into(),
                from: "Sam".into(),
                passage: passage(),
                note: "Please review".into(),
            }),
            dropped("sg-1"),
        ],
        "the new holder removes the card it was sent"
    );
}

fn assert_sender_sees_sent_then_dropped(to_sender: Vec<String>) {
    assert_eq!(
        parse(&to_sender),
        vec![
            PlanMessage::Review(ReviewMessage::SuggestionStatus {
                id: "sg-1".into(),
                status: SuggestionStatus::Sent {
                    holder: "Alex".into(),
                    away: false,
                },
            }),
            dropped("sg-1"),
        ],
        "the sender is told once"
    );
}

#[test]
fn new_holder_drops_a_card_sent_while_authoring_changed_hands() {
    let sender_browser = BrowserId::mint().unwrap();
    let armed = Arc::new(AtomicBool::new(false));
    let f = {
        let (sender, armed) = (sender_browser.clone(), Arc::clone(&armed));
        Fixture::with_early_subscriber(move |ctx, event| {
            // The holder is already the new browser when this suggestion arrives, so the card
            // reaches it before review's drop runs.
            if matches!(event, AuthoringEvent::Taken { .. }) && armed.load(Ordering::SeqCst) {
                dispatch(ctx, &ctx.conn_ctx(sender.clone()), suggest_command());
            }
        })
    };
    f.ctx.presence().set_name(&sender_browser, "Sam");
    let sender = f.tab(&sender_browser);
    let holder = f.connect("Alex");
    let bystander = f.connect("Dana");
    armed.store(true, Ordering::SeqCst);

    f.ctx
        .require_authoring(&holder.conn.browser, Take::TakeIfFree)
        .unwrap();

    let (to_holder, to_sender, to_bystander) = (holder.raw(), sender.raw(), bystander.raw());
    for raw in [&to_holder, &to_sender, &to_bystander] {
        assert_no_cookies(raw, &[&holder, &sender, &bystander]);
    }
    assert_holder_sees_card_then_drop(to_holder);
    assert_sender_sees_sent_then_dropped(to_sender);
    assert!(to_bystander.is_empty());
    let state = f.ctx.extensions().get::<ReviewState>().unwrap();
    assert_eq!(state.suggestions.lock().unwrap().pending(), 0);
    assert!(state.senders.lock().unwrap().is_empty());
}

#[test]
fn a_sender_who_becomes_the_holder_is_told_once() {
    let sender_browser = BrowserId::mint().unwrap();
    let armed = Arc::new(AtomicBool::new(false));
    let f = {
        let (sender, armed) = (sender_browser.clone(), Arc::clone(&armed));
        Fixture::with_early_subscriber(move |ctx, event| {
            // The sender takes the freed authoring before review's drop for the release runs.
            if matches!(event, AuthoringEvent::Freed { .. }) && armed.swap(false, Ordering::SeqCst)
            {
                ctx.require_authoring(&sender, Take::TakeIfFree).unwrap();
            }
        })
    };
    f.ctx.presence().set_name(&sender_browser, "Sam");
    let sender = f.tab(&sender_browser);
    let holder = f.connect("Dana");
    f.ctx
        .require_authoring(&holder.conn.browser, Take::TakeIfFree)
        .unwrap();
    dispatch(&f.ctx, &sender.conn, suggest_command());
    sender.frames();
    armed.store(true, Ordering::SeqCst);

    f.ctx.authoring().release(FreeReason::Released("test"));

    assert!(f.ctx.authoring().is_holder(&sender_browser));
    let to_sender = sender.raw();
    assert_no_cookies(&to_sender, &[&holder, &sender]);
    assert_eq!(parse(&to_sender), vec![dropped("sg-1")]);
}
