//! The `doc/` watcher feeding the project view, and the 1 s housekeeping tick.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use super::context::PlanContext;
use super::wire::checklist::ChecklistView;
use super::wire::message::PlanMessage;
use crate::planning::watch::{Batch, Watcher};

/// How often the watcher walks `doc/`, and how often the tick runs.
const INTERVAL: Duration = Duration::from_secs(1);
/// Longest single sleep of the tick, so a stop request is seen promptly.
const STOP_CHECK: Duration = Duration::from_millis(50);

/// The watcher's failed-walk message, shared with the tick that reports it.
pub type ErrorSlot = Arc<Mutex<Option<String>>>;

/// Start watching `ctx.root()/doc/` until `stop` is set. Every batch after the initial one
/// refreshes the project, runs the `on_batch` hooks in order, and broadcasts the checklist
/// when it changed. Returns the thread and the watcher's error slot, which `Watcher::spawn`
/// would otherwise hide: hand it to [`spawn_tick`].
pub fn spawn_watcher(
    ctx: Arc<PlanContext>,
    stop: Arc<AtomicBool>,
) -> Result<(JoinHandle<()>, ErrorSlot)> {
    let watcher = Watcher::new(ctx.root().to_path_buf(), ctx.own_writes().clone());
    let errors = watcher.error_slot();
    let thread = watcher.spawn(INTERVAL, stop, move |batch| apply_batch(&ctx, &batch))?;
    Ok((thread, errors))
}

fn apply_batch(ctx: &PlanContext, batch: &Batch) {
    if batch.initial {
        return;
    }
    let changed = {
        let mut project = ctx.project_mut();
        let before = ChecklistView::from(&project.checklist);
        if let Err(error) = project.refresh(batch) {
            tracing::warn!("loom plan could not refresh the project: {error:#}");
        }
        let after = ChecklistView::from(&project.checklist);
        (before != after).then_some(after)
    };
    for hook in ctx.batch_hooks() {
        hook(ctx, batch);
    }
    if let Some(checklist) = changed {
        ctx.hub().broadcast(&PlanMessage::Checklist(checklist));
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
