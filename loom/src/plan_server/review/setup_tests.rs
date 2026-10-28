//! Tests for the review setup hooks: reconnect replay and open-question broadcasts.

use std::collections::BTreeMap;
use std::sync::{mpsc, Arc};
use std::time::SystemTime;

use super::commands::dispatch;
use crate::plan_server::authoring::Take;
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, ConnCtx};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::review::{ReviewCommand, ReviewMessage, SuggestedPassageWire};
use crate::planning::watch::{Batch, OwnWrites};

const SECTION: &str = "doc/product/section_01.md";

struct Fixture {
    _dir: tempfile::TempDir,
    ctx: Arc<PlanContext>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("doc/product")).unwrap();
        std::fs::write(dir.path().join("doc/product/index.md"), "# Demo\n").unwrap();
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
    fn frames(&self) -> Vec<PlanMessage> {
        self.rx
            .try_iter()
            .map(|frame| serde_json::from_str(&frame).unwrap())
            .collect()
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

    f.edit_batch("doc/product/index.md", "# Demo\n\nUpdated overview.\n");

    assert!(first.frames().is_empty());
    assert!(second.frames().is_empty());
}
