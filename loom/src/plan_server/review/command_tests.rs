//! Tests for the review WebSocket commands.

use super::commands::*;
use super::state::ReviewState;
use crate::plan_server::authoring::{FreeReason, Take};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, ConnCtx};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::review::{ReviewCommand, SuggestedPassageWire, SuggestionActionWire};
use crate::planning::watch::OwnWrites;
use serde_json::{json, Value};
use std::sync::{mpsc, Arc};
use std::time::SystemTime;

struct Fixture {
    _dir: tempfile::TempDir,
    ctx: Arc<PlanContext>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("doc/product/demo")).unwrap();
        std::fs::write(dir.path().join("doc/product/demo/index.md"), "# Demo\n").unwrap();
        std::fs::write(
            dir.path().join("doc/product/demo/section_01.md"),
            "# Section 1\n",
        )
        .unwrap();
        // On disk, but discovery skips dot-names, so no suggestion may name it.
        std::fs::create_dir_all(dir.path().join("doc/.hidden")).unwrap();
        std::fs::write(dir.path().join("doc/.hidden/x.md"), "# Hidden\n").unwrap();
        let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
        let ctx = PlanContext::new(
            dir.path().to_path_buf(),
            ReadOnly::from_problems(vec![]),
            bind,
            OwnWrites::default(),
        );
        let ctx = Arc::new(ctx.unwrap());
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

    fn take(&self, tab: &Tab) {
        self.ctx
            .require_authoring(&tab.conn.browser, Take::TakeIfFree)
            .unwrap();
    }

    fn state(&self) -> Arc<ReviewState> {
        self.ctx.extensions().get::<ReviewState>().unwrap()
    }
}

struct Tab {
    conn: ConnCtx,
    rx: mpsc::Receiver<Arc<String>>,
}

impl Tab {
    fn frames(&self) -> Vec<Value> {
        self.rx
            .try_iter()
            .map(|frame| serde_json::from_str(&frame).unwrap())
            .collect()
    }
}

fn passage() -> SuggestedPassageWire {
    SuggestedPassageWire {
        path: "doc/product/demo/section_01.md".into(),
        anchor: "goals".into(),
        text: "Keep one author at a time.".into(),
        lines: Some((3, 4)),
        stage: Some("review".into()),
        summary_sentence: Some("One author".into()),
    }
}

fn suggest() -> ReviewCommand {
    ReviewCommand::Suggest {
        passage: passage(),
        note: "Please review".into(),
    }
}

fn assert_no_cookies(frames: &[Value], tabs: &[&Tab]) {
    for frame in frames {
        for tab in tabs {
            assert!(!frame.to_string().contains(tab.conn.browser.as_str()));
        }
    }
}

#[test]
fn suggest_from_the_holder_refused() {
    let f = Fixture::new();
    let holder = f.connect("Dana");
    let sibling = f.tab(&holder.conn.browser);
    f.take(&holder);

    dispatch(&f.ctx, &holder.conn, suggest());

    assert_eq!(
        holder.frames(),
        vec![json!({
            "type": "review", "kind": "refused", "reason": "This browser holds authoring"
        })]
    );
    assert!(
        sibling.frames().is_empty(),
        "refusal belongs to the requesting tab"
    );
    assert_eq!(f.state().suggestions.lock().unwrap().pending(), 0);
    assert!(f.state().senders.lock().unwrap().is_empty());
}

#[test]
fn status_delivered_to_the_sender_and_every_holder_tab() {
    for (action, expected) in [
        (SuggestionActionWire::Attach, "attached"),
        (SuggestionActionWire::Dismiss, "dismissed"),
    ] {
        let f = Fixture::new();
        let holder = f.connect("Dana");
        let holder_sibling = f.tab(&holder.conn.browser);
        let sender = f.connect("Sam");
        let sibling = f.tab(&sender.conn.browser);
        let observer = f.connect("Alex");
        f.take(&holder);
        dispatch(&f.ctx, &sender.conn, suggest());
        assert_eq!(holder.frames().len(), 1, "the holder gets the suggestion");
        assert_eq!(holder_sibling.frames().len(), 1, "every holder tab gets it");
        let sent = sender.frames();
        assert_eq!(sibling.frames(), sent);
        let id = sent[0]["id"].as_str().unwrap().to_owned();
        let command = ReviewCommand::SuggestionAnswer {
            id: id.clone(),
            action,
        };

        dispatch(&f.ctx, &holder.conn, command);

        let frames = sender.frames();
        assert_eq!(
            frames,
            vec![json!({
                "type": "review", "kind": "suggestion_status", "id": id, "state": expected
            })]
        );
        assert_eq!(sibling.frames(), frames);
        // The holder's web side removes the card only on this status.
        assert_eq!(holder.frames(), frames, "the answering tab gets the status");
        assert_eq!(
            holder_sibling.frames(),
            frames,
            "sibling holder tabs drop the card"
        );
        assert!(observer.frames().is_empty());
        assert_no_cookies(&frames, &[&holder, &sender, &observer]);
        assert!(f.state().senders.lock().unwrap().is_empty());
        assert_eq!(f.state().suggestions.lock().unwrap().pending(), 0);
    }
}

#[test]
fn sent_status_carries_away() {
    for away in [false, true] {
        let f = Fixture::new();
        let holder = f.connect("Dana");
        let sender = f.connect("Sam");
        f.take(&holder);
        if away {
            f.ctx.hub().unregister(holder.conn.id);
            f.ctx
                .authoring()
                .connection_closed(&holder.conn.browser, SystemTime::UNIX_EPOCH);
        }

        dispatch(&f.ctx, &sender.conn, suggest());

        let frames = sender.frames();
        assert_eq!(
            frames,
            vec![json!({
                "type": "review", "kind": "suggestion_status", "id": "sg-1",
                "state": "sent", "holder": "Dana", "away": away
            })]
        );
        assert_no_cookies(&frames, &[&holder, &sender]);
    }
}

#[test]
fn dropped_suggestions_notify_senders() {
    let f = Fixture::new();
    let holder = f.connect("Dana");
    let first = f.connect("Sam");
    let second = f.connect("Alex");
    let sibling = f.tab(&first.conn.browser);
    f.take(&holder);
    dispatch(&f.ctx, &first.conn, suggest());
    dispatch(&f.ctx, &second.conn, suggest());
    first.frames();
    second.frames();
    sibling.frames();
    assert_eq!(holder.frames().len(), 2, "the holder gets both suggestions");

    f.ctx.authoring().release(FreeReason::Released("test"));
    f.take(&second);

    let first_frames = first.frames();
    let second_frames = second.frames();
    for (frames, id) in [(&first_frames, "sg-1"), (&second_frames, "sg-2")] {
        assert_eq!(
            *frames,
            vec![json!({
                "type": "review", "kind": "suggestion_status", "id": id,
                "state": "dropped", "reason": "Authoring holder changed."
            })]
        );
        assert_no_cookies(frames, &[&holder, &first, &second]);
    }
    assert_eq!(sibling.frames(), first_frames);
    assert!(holder.frames().is_empty());
    assert!(f.state().senders.lock().unwrap().is_empty());
    assert_eq!(f.state().suggestions.lock().unwrap().pending(), 0);
}

#[test]
fn suggestion_reaches_the_holders_open_tab_live() {
    let f = Fixture::new();
    let holder = f.connect("Dana");
    let sibling = f.tab(&holder.conn.browser);
    let sender = f.connect("Sam");
    let observer = f.connect("Alex");
    f.take(&holder);

    dispatch(&f.ctx, &sender.conn, suggest());

    let expected = vec![json!({
        "type": "review", "kind": "suggestion", "id": "sg-1", "from": "Sam",
        "passage": passage(), "note": "Please review"
    })];
    let frames = holder.frames();
    assert_eq!(frames, expected);
    assert_eq!(sibling.frames(), expected);
    assert_no_cookies(&frames, &[&holder, &sender, &observer]);
    assert!(observer.frames().is_empty());
    assert!(!sender
        .frames()
        .iter()
        .any(|frame| frame["kind"] == "suggestion"));
}

#[test]
fn pending_suggestion_preserves_passage_and_sender_display_name() {
    let f = Fixture::new();
    let holder = f.connect("Dana");
    let sender = f.connect("Sam");
    f.take(&holder);

    dispatch(&f.ctx, &sender.conn, suggest());

    let frames: Vec<Value> = f
        .ctx
        .hub()
        .open_frames(&holder.conn)
        .iter()
        .map(|frame| serde_json::from_str(frame).unwrap())
        .collect();
    assert_eq!(
        frames,
        vec![json!({
            "type": "review", "kind": "suggestion", "id": "sg-1", "from": "Sam",
            "passage": passage(), "note": "Please review"
        })]
    );
    assert!(f.ctx.hub().open_frames(&sender.conn).is_empty());
    assert_no_cookies(&frames, &[&holder, &sender]);
    assert_eq!(
        f.state().senders.lock().unwrap().get("sg-1"),
        Some(&sender.conn.browser),
    );
}

#[test]
fn answer_from_a_viewer_is_refused_without_consuming_the_suggestion() {
    let f = Fixture::new();
    let holder = f.connect("Dana");
    let sender = f.connect("Sam");
    f.take(&holder);
    dispatch(&f.ctx, &sender.conn, suggest());
    sender.frames();

    dispatch(
        &f.ctx,
        &sender.conn,
        ReviewCommand::SuggestionAnswer {
            id: "sg-1".into(),
            action: SuggestionActionWire::Attach,
        },
    );

    assert_eq!(
        sender.frames(),
        vec![json!({
            "type": "review", "kind": "refused",
            "reason": "Only the author can answer a suggestion"
        })]
    );
    assert_eq!(f.state().suggestions.lock().unwrap().pending(), 1);
    assert!(f.state().senders.lock().unwrap().contains_key("sg-1"));
}

#[test]
fn missing_holder_and_unknown_suggestion_have_refusal_text() {
    let f = Fixture::new();
    let sender = f.connect("Sam");

    dispatch(&f.ctx, &sender.conn, suggest());
    assert_eq!(
        sender.frames(),
        vec![json!({
            "type": "review", "kind": "refused", "reason": "Nobody holds authoring"
        })]
    );
    f.take(&sender);
    dispatch(
        &f.ctx,
        &sender.conn,
        ReviewCommand::SuggestionAnswer {
            id: "missing".into(),
            action: SuggestionActionWire::Dismiss,
        },
    );
    assert_eq!(
        sender.frames(),
        vec![json!({
            "type": "review", "kind": "refused", "reason": "Unknown suggestion"
        })]
    );
}

#[test]
fn suggestion_for_an_undiscovered_path_is_refused_without_a_slot() {
    for path in ["doc/.hidden/x.md", "doc/product/demo/missing.md"] {
        let f = Fixture::new();
        let holder = f.connect("Dana");
        let sender = f.connect("Sam");
        f.take(&holder);
        let mut bad = passage();
        bad.path = path.into();

        dispatch(
            &f.ctx,
            &sender.conn,
            ReviewCommand::Suggest {
                passage: bad,
                note: "Please review".into(),
            },
        );

        let reason = format!("Invalid suggestion path: {path} is not a discovered document");
        assert_eq!(
            sender.frames(),
            vec![json!({ "type": "review", "kind": "refused", "reason": reason })],
            "{path}"
        );
        assert!(holder.frames().is_empty(), "{path}");
        assert_eq!(f.state().suggestions.lock().unwrap().pending(), 0);
        assert!(f.state().senders.lock().unwrap().is_empty());
    }
}
