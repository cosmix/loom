//! The execution strip shared with planning clients.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::commands::status::{data::collect_status_data, render::attention_entries};
use crate::daemon::DaemonServer;
use crate::fs::{plan_lifecycle::get_plan_source_path, work_dir::WorkDir};
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle;
use crate::plan_server::wire::{message::PlanMessage, run::RunStrip};

const INTERVAL: Duration = Duration::from_secs(5);
const STOP_CHECK: Duration = Duration::from_millis(50);

/// Collect a snapshot suitable for broadcasting, without dashboard credentials.
pub fn build_strip(work_dir: &WorkDir, dashboard_url: Option<&str>) -> anyhow::Result<RunStrip> {
    if !work_dir.root().try_exists()? {
        return Ok(empty_strip());
    }
    let data = collect_status_data(work_dir)?;
    Ok(RunStrip {
        plan: get_plan_source_path(work_dir)?.map(|path| path.to_string_lossy().into_owned()),
        daemon_running: DaemonServer::is_running(work_dir.root()),
        merged: u32::try_from(data.merge.merged.len())?,
        total: u32::try_from(data.progress.total)?,
        attention: u32::try_from(attention_entries(&data.stages).len())?,
        dashboard_url: dashboard_url.map(super::dashboard_parse::strip_token),
    })
}

fn empty_strip() -> RunStrip {
    RunStrip {
        plan: None,
        daemon_running: false,
        merged: 0,
        total: 0,
        attention: 0,
        dashboard_url: None,
    }
}

/// Errors leave the caller's last broadcast untouched.
fn changed_strip(
    last: Option<&RunStrip>,
    next: anyhow::Result<RunStrip>,
) -> anyhow::Result<Option<RunStrip>> {
    let next = next?;
    Ok((last != Some(&next)).then_some(next))
}

pub(super) fn spawn_strip(ctx: Arc<PlanContext>) {
    let spawned = thread::Builder::new()
        .name("loom-plan-run-strip".into())
        .spawn(move || poll_strip(&ctx));
    if let Err(error) = spawned {
        lifecycle::line(&format!(
            "run strip: cannot start poll thread: {}",
            lifecycle::safe(&error.to_string())
        ));
    }
}

fn poll_strip(ctx: &PlanContext) {
    let mut last = None;
    let mut reported_error = false;
    while !ctx.is_stopping() {
        let next = WorkDir::new(ctx.root()).and_then(|work_dir| {
            let url = super::dashboard::current_url(ctx);
            build_strip(&work_dir, url.as_deref())
        });
        match changed_strip(last.as_ref(), next) {
            Ok(next) => {
                reported_error = false;
                if let Some(strip) = next {
                    ctx.hub().broadcast(&PlanMessage::RunStrip(strip.clone()));
                    last = Some(strip);
                }
            }
            Err(error) if !reported_error => {
                reported_error = true;
                lifecycle::line(&format!(
                    "run strip: cannot read status: {}",
                    lifecycle::safe(&error.to_string())
                ));
            }
            Err(_) => {}
        }
        sleep_unless_stopped(ctx);
    }
}

fn sleep_unless_stopped(ctx: &PlanContext) {
    let deadline = Instant::now() + INTERVAL;
    while !ctx.is_stopping() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        thread::sleep(left.min(STOP_CHECK));
    }
}

#[cfg(test)]
mod tests;
