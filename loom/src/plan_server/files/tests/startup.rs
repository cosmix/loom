//! The startup validation pass and the batches that arrive while it runs, observed through a
//! connection registered on the hub (no socket, so nothing is missed before it opens).

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;

use super::doc::{context, write_files};
use super::validate::{plan, SELF_DEPENDENCY};
use crate::plan_server::context::PlanContext;
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::files::validate::ValidationCache;
use crate::plan_server::files::FileMode;
use crate::plan_server::ids::{BrowserId, ConnCtx, ConnId, PublicId};
use crate::plan_server::tests::support::WAIT;
use crate::plan_server::wire::files::{FileTree, TreeEntry};
use crate::plan_server::wire::message::PlanMessage;
use crate::planning::watch::Batch;

const GOOD: &str = "doc/plans/PLAN-good.md";
const BAD: &str = "doc/plans/PLAN-bad.md";
const DONE: &str = "doc/plans/DONE-PLAN-old.md";

/// A context over a project with a valid plan, an invalid one and a locked one.
fn project() -> (tempfile::TempDir, PlanContext) {
    let dir = write_files(&[
        (GOOD, &plan(".", "true", "")),
        (BAD, &plan(".", "true", SELF_DEPENDENCY)),
        (DONE, "# Old\n"),
    ]);
    let ctx = context(&dir);
    (dir, ctx)
}

fn file_mode(ctx: &PlanContext) -> FileMode {
    let texts = ctx.extensions().get::<TextStore>().expect("text store");
    FileMode::new(texts, Arc::new(ValidationCache::default()))
}

/// What a browser connected from now on would be sent.
fn subscribe(ctx: &PlanContext) -> Receiver<Arc<String>> {
    let conn = ConnCtx {
        id: ConnId(1),
        browser: BrowserId::mint().expect("browser id"),
        public_id: PublicId::mint(),
    };
    ctx.hub().register(&conn)
}

fn parse(frame: &str) -> PlanMessage {
    serde_json::from_str(frame).expect("a planning message")
}

fn queued(frames: &Receiver<Arc<String>>) -> Vec<PlanMessage> {
    frames.try_iter().map(|frame| parse(&frame)).collect()
}

/// The `(path, valid)` of every `validation` message, sorted; panics on any other kind but the
/// one `tree_changed` the pass ends with, which it returns.
fn validations_and_tree(sent: &[PlanMessage]) -> (Vec<(&str, bool)>, &FileTree) {
    let mut validated = Vec::new();
    let mut trees = Vec::new();
    for message in sent {
        match message {
            PlanMessage::Validation(v) => validated.push((v.path.as_str(), v.validation.valid)),
            PlanMessage::TreeChanged(tree) => trees.push(tree),
            other => panic!("unexpected {other:?} in {sent:?}"),
        }
    }
    validated.sort_unstable();
    assert_eq!(trees.len(), 1, "one tree for the pass: {sent:?}");
    assert!(
        matches!(sent.last(), Some(PlanMessage::TreeChanged(_))),
        "the tree follows every validation: {sent:?}"
    );
    (validated, trees[0])
}

/// The error count the tree shows for the plan at `path`.
fn tree_errors(tree: &FileTree, path: &str) -> Option<u32> {
    fn find<'a>(entries: &'a [TreeEntry], path: &str) -> Option<&'a TreeEntry> {
        entries.iter().find_map(|entry| {
            let own = entry.path.as_deref() == Some(path);
            own.then_some(entry).or_else(|| find(&entry.children, path))
        })
    }
    find(&tree.plans, path).map(|entry| entry.errors)
}

#[test]
fn the_pass_broadcasts_each_unlocked_plan_then_the_tree_with_error_counts() {
    let (_dir, ctx) = project();
    let frames = subscribe(&ctx);
    file_mode(&ctx).validate_all(&ctx);
    let sent = queued(&frames);
    let (validated, tree) = validations_and_tree(&sent);
    assert_eq!(
        validated,
        [(BAD, false), (GOOD, true)],
        "the locked plan has none"
    );
    assert_eq!(tree_errors(tree, GOOD), Some(0));
    assert!(tree_errors(tree, BAD).is_some_and(|errors| errors > 0));
}

#[test]
fn a_batch_during_the_pass_is_applied_and_the_pass_results_stay() {
    let (dir, ctx) = project();
    let frames = subscribe(&ctx);
    let state = file_mode(&ctx);
    // The batch breaks the valid plan; the pass alone would leave it valid.
    std::fs::write(dir.path().join(GOOD), plan(".", "true", SELF_DEPENDENCY)).unwrap();
    let batch = Batch {
        changed: vec![PathBuf::from(GOOD)],
        ..Batch::default()
    };
    let mut sent = Vec::new();
    // The pass cannot read the project while this lock is held, so either it stalls holding the
    // `published` lock or the batch does: both are in flight until the lock is released.
    let stall = ctx.project_mut();
    thread::scope(|scope| {
        let pass = scope.spawn(|| state.validate_all(&ctx));
        let follow = scope.spawn(|| state.follow(&ctx, &batch));
        // Sent by the batch before it waits for the pass, so the pass has sent nothing yet.
        let first = frames.recv_timeout(WAIT).expect("the batch's doc_changed");
        sent.push(parse(&first));
        drop(stall);
        pass.join().unwrap();
        follow.join().unwrap();
    });
    assert!(matches!(&sent[0], PlanMessage::DocChanged(c) if c.path == GOOD));
    sent.extend(queued(&frames));
    let (validated, tree) = validations_and_tree(&sent[1..]);
    assert_eq!(validated, [(BAD, false), (GOOD, false)], "{sent:?}");
    assert!(tree_errors(tree, GOOD).is_some_and(|errors| errors > 0));
    assert!(tree_errors(tree, BAD).is_some_and(|errors| errors > 0));
    for rel in [GOOD, BAD] {
        let view = state.cache.view(Path::new(rel)).expect("cached");
        assert!(!view.valid, "{rel}: {view:?}");
    }
}

#[test]
fn a_stopping_server_ends_the_pass_before_validating_or_publishing() {
    let (_dir, ctx) = project();
    let frames = subscribe(&ctx);
    ctx.shutdown().begin(|_| {}).join().expect("shutdown");
    let state = file_mode(&ctx);
    state.validate_all(&ctx);
    assert!(queued(&frames).is_empty());
    assert!(state.cache.error_counts().is_empty());
}
