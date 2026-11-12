//! The `doc/` watcher feeding the project view, and the 1 s housekeeping tick.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use super::context::PlanContext;
use super::trace;
use super::wire::message::PlanMessage;
use crate::planning::watch::{Batch, OwnWrites, Watcher};

/// How often the watcher walks `doc/`, and how often the tick runs.
const INTERVAL: Duration = Duration::from_secs(1);
/// Longest single sleep of the tick, so a stop request is seen promptly.
const STOP_CHECK: Duration = Duration::from_millis(50);

/// The watcher's failed-walk message, shared with the tick that reports it.
pub type ErrorSlot = Arc<Mutex<Option<String>>>;

/// A watcher over `root/doc/` that has taken its baseline walk (the `initial` batch, which
/// lists what is already there). Take it before the project and the stores load: a document
/// changed after this walk reaches them as a later batch, so an edit made while they load is
/// not lost; one they already read is applied again, which is harmless. A failed walk leaves
/// the baseline to the watcher's first poll, whose `initial` batch is then dropped.
pub fn baseline(root: PathBuf, own_writes: OwnWrites) -> Watcher {
    let mut watcher = Watcher::new(root, own_writes);
    if let Err(error) = watcher.poll() {
        tracing::warn!("loom plan could not take the doc/ baseline: {error:#}");
    }
    watcher
}

/// Poll `watcher` (from [`baseline`], sharing `ctx.own_writes()`) every second until `stop`
/// is set. Every batch after the initial one refreshes the project, runs the `on_batch` hooks
/// in order, and broadcasts the planning snapshot when it changed. Returns the thread and the
/// watcher's error slot, which `Watcher::spawn` would otherwise hide: hand it to
/// [`spawn_tick`].
pub fn spawn_watcher(
    ctx: Arc<PlanContext>,
    watcher: Watcher,
    stop: Arc<AtomicBool>,
) -> Result<(JoinHandle<()>, ErrorSlot)> {
    let errors = watcher.error_slot();
    let thread = watcher.spawn(INTERVAL, stop, move |batch| apply_batch(&ctx, &batch))?;
    Ok((thread, errors))
}

pub(super) fn apply_batch(ctx: &PlanContext, batch: &Batch) {
    if batch.initial {
        return;
    }
    let (before, traced) = {
        let mut project = ctx.project_mut();
        let before = ctx.checklist_of(&project);
        let traces_before = trace::changed::views(&project);
        if let Err(error) = project.refresh(batch) {
            tracing::warn!("loom plan could not refresh the project: {error:#}");
        }
        let traces_after = trace::changed::views(&project);
        (
            before,
            trace::changed::changed(&traces_before, &traces_after),
        )
    };
    for hook in ctx.batch_hooks() {
        hook(ctx, batch);
    }
    // Compared after the hooks: the summary engine's hook updates the evaluation that
    // `has_summary` reads, so a sealed summary reaches the standalone table.
    let after = ctx.checklist_of(&ctx.project());
    if before != after {
        ctx.hub().broadcast(&PlanMessage::Checklist(after));
    }
    for feature in traced {
        ctx.hub().broadcast(&PlanMessage::TraceChanged { feature });
    }
}

/// Every second until `stop` is set: free authoring whose grace expired, and log the
/// watcher's failed-walk message once each time it changes.
pub fn spawn_tick(
    ctx: Arc<PlanContext>,
    errors: ErrorSlot,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("loom-plan-tick".into())
        .spawn(move || {
            let mut reported: Option<String> = None;
            while !stop.load(Ordering::SeqCst) {
                ctx.authoring().tick(SystemTime::now());
                report_walk_error(&errors, &mut reported);
                sleep_unless_stopped(&stop);
            }
        })
        .context("cannot start the loom plan tick thread")
}

fn report_walk_error(errors: &ErrorSlot, reported: &mut Option<String>) {
    let current = errors
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if current == *reported {
        return;
    }
    if let Some(message) = &current {
        tracing::warn!("loom plan cannot read doc/; the project view is stale: {message}");
    }
    *reported = current;
}

fn sleep_unless_stopped(stop: &AtomicBool) {
    let deadline = Instant::now() + INTERVAL;
    while !stop.load(Ordering::SeqCst) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        thread::sleep(left.min(STOP_CHECK));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_server::context::BindInfo;
    use crate::plan_server::ids::BrowserId;
    use crate::plan_server::prereq::ReadOnly;
    use crate::plan_server::wire::checklist::ProjectChecklistView;
    use serde_json::{json, Value};
    use std::path::Path;

    fn put(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn context(root: &Path) -> Arc<PlanContext> {
        Arc::new(
            PlanContext::new(
                root.to_path_buf(),
                ReadOnly::from_problems(Vec::new()),
                BindInfo::new("127.0.0.1:7272".parse().unwrap(), "token"),
                OwnWrites::default(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn standalone_title_change_broadcasts_even_without_feature_checklists() {
        let root = tempfile::tempdir().unwrap();
        put(root.path(), "doc/plans/PLAN-fix.md", "# Before\n");
        let ctx = context(root.path());
        let conn = ctx.conn_ctx(BrowserId::parse(&"a".repeat(32)).unwrap());
        let frames = ctx.hub().register(&conn);
        put(root.path(), "doc/plans/PLAN-fix.md", "# After\n");
        let batch = Batch {
            changed: vec!["doc/plans/PLAN-fix.md".into()],
            ..Batch::default()
        };

        apply_batch(&ctx, &batch);

        let frame = frames.try_recv().expect("the standalone shelf changed");
        let mut value: Value = serde_json::from_str(&frame).unwrap();
        let modified = value["standalone"][0]
            .as_object_mut()
            .unwrap()
            .remove("last_change")
            .unwrap();
        assert!(modified.as_str().unwrap().ends_with('Z'));
        assert_eq!(
            value,
            json!({
                "type": "checklist", "checklists": [], "moves": [],
                "standalone": [{"slug":"fix", "path":"doc/plans/PLAN-fix.md", "state":"written", "title":"After", "pressure_date": null, "has_summary": false}]
            })
        );
        apply_batch(&ctx, &batch);
        assert!(
            frames.try_recv().is_err(),
            "an unchanged snapshot is not broadcast"
        );
    }

    #[test]
    fn misplaced_document_broadcasts_its_move_offer_without_an_implicit_feature() {
        let root = tempfile::tempdir().unwrap();
        let ctx = context(root.path());
        let conn = ctx.conn_ctx(BrowserId::parse(&"b".repeat(32)).unwrap());
        let frames = ctx.hub().register(&conn);
        put(root.path(), "doc/product/index.md", "# Misplaced\n");

        apply_batch(
            &ctx,
            &Batch {
                added: vec!["doc/product/index.md".into()],
                ..Batch::default()
            },
        );

        let frame = frames.try_recv().expect("the move offers changed");
        let value: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(
            value,
            json!({
                "type":"checklist", "checklists":[], "standalone":[],
                "moves":[{
                    "id":"move-into-feature:doc/product/index.md", "kind":"move_into_feature",
                    "label":"Move doc/product/index.md into a feature",
                    "blocked_reason":null, "target":"doc/product/index.md"
                }]
            })
        );
    }

    #[test]
    fn hello_contains_the_same_feature_snapshot_as_checklist_updates() {
        let root = tempfile::tempdir().unwrap();
        put(root.path(), "doc/product/zeta/index.md", "# Zeta\n");
        put(root.path(), "doc/plans/alpha/index.md", "# Alpha\n");
        put(root.path(), "doc/plans/PLAN-fix.md", "# Fix\n");
        let ctx = context(root.path());
        PlanContext::install_hub_hooks(&ctx);
        let conn = ctx.conn_ctx(BrowserId::parse(&"c".repeat(32)).unwrap());

        let frames = ctx.hub().open_frames(&conn);

        let PlanMessage::Hello(hello) = serde_json::from_str(&frames[0]).unwrap() else {
            panic!("the first frame must be hello");
        };
        let snapshot = ProjectChecklistView::from(&*ctx.project());
        assert_eq!(hello.checklists, snapshot.checklists);
        assert_eq!(hello.standalone, snapshot.standalone);
        assert_eq!(hello.moves, snapshot.moves);
    }
}
