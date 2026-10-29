//! Starting a run with the running loom binary and streamed child output.

use std::io::{self, ErrorKind};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Weak};
use std::thread;

use super::child::{self, JobGuard};
use super::state::orphaned::relative_work_dir;
use crate::fs::work_dir::WorkDir;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::shutdown::ShutdownHook;
use crate::plan_server::wire::run::{
    Backend, OutputStream, RunJob, RunStartRequest, RunStateView, WorkspaceState,
};

/// Give borrowed HTTP requests an owned context for their worker thread.
pub(crate) fn install(ctx: &Arc<PlanContext>) {
    ctx.extensions().insert(Arc::new(Arc::downgrade(ctx)));
}

/// The commands an admitted start runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Steps {
    /// `loom init <plan>`, then `loom run`.
    InitThenRun,
    /// `loom run` alone: the workspace already holds the selected plan, which init refuses.
    RunOnly,
}

/// Decide from the state view what a start runs, or the refusal code (section 11's offers).
fn admit(state: &RunStateView, request: &RunStartRequest) -> Result<Steps, &'static str> {
    if !state.blocking_changes.is_empty() {
        return Err("blocked");
    }
    let holds_selected = state.active_plan.as_deref() == Some(request.plan.as_str());
    match state.workspace {
        // Resume. Its IN_PROGRESS- plan is not a written plan, so it precedes that check.
        WorkspaceState::Stopped if holds_selected && !request.clean => return Ok(Steps::RunOnly),
        WorkspaceState::Running => return Err("running"),
        // `loom init --clean` would delete every stage branch without abandon's confirmation.
        WorkspaceState::Stopped => return Err("stopped"),
        WorkspaceState::Absent | WorkspaceState::Initialized | WorkspaceState::Done => {}
    }
    if !state.plans.iter().any(|plan| plan.path == request.plan) {
        return Err("not_written");
    }
    match state.workspace {
        WorkspaceState::Initialized if holds_selected && !request.clean => Ok(Steps::RunOnly),
        WorkspaceState::Initialized | WorkspaceState::Done if !request.clean => Err("needs_clean"),
        _ => Ok(Steps::InitThenRun),
    }
}

const TRACKED_CHANGES: &str = "commit or stash tracked changes outside doc/ before starting";

/// Why a start is blocked: the plan-less workspace's own entry names its remedy (`loom clean
/// --all`), which the advice about tracked changes would hide.
pub(super) fn blocked_message<'a>(root: &Path, state: &'a RunStateView) -> &'a str {
    let run_state = WorkDir::new(root)
        .ok()
        .map(|work_dir| relative_work_dir(root, &work_dir));
    state
        .blocking_changes
        .iter()
        .find(|change| run_state.as_deref() == Some(change.path.as_str()))
        .map_or(TRACKED_CHANGES, |change| change.status.as_str())
}

pub(super) fn post_start(req: &ApiRequest<'_>) -> ApiResponse {
    match accept_start(req) {
        Ok(()) => ApiResponse::json(202, &serde_json::json!({ "accepted": true })),
        Err(response) => response,
    }
}

fn accept_start(req: &ApiRequest<'_>) -> Result<(), ApiResponse> {
    if req.ctx.read_only().is_read_only() {
        return Err(ApiResponse::error(
            403,
            "read_only",
            "loom plan is in read-only mode",
        ));
    }
    let request: RunStartRequest = req.json()?;
    let state = super::state::state_view(req.ctx)
        .map_err(|error| ApiResponse::error(500, "internal", &error.to_string()))?;
    let steps = admit(&state, &request).map_err(|code| {
        let message = match code {
            "blocked" => blocked_message(req.ctx.root(), &state),
            "not_written" => "the selected plan is not a written plan",
            "running" => "a plan is already running",
            "stopped" => "a stopped run holds the workspace: resume it or abandon it",
            _ => "starting this plan requires --clean",
        };
        ApiResponse::error(409, code, message)
    })?;
    let guard = child::jobs(req.ctx)
        .begin(RunJob::Start)
        .ok_or_else(|| ApiResponse::error(409, "busy", "another run job is in progress"))?;
    let ctx = req
        .ctx
        .extensions()
        .get::<Weak<PlanContext>>()
        .and_then(|weak| weak.upgrade())
        .ok_or_else(|| ApiResponse::error(500, "internal", "run start is not installed"))?;
    let loom_bin = std::env::current_exe()
        .map_err(|error| ApiResponse::error(500, "internal", &error.to_string()))?;
    let worker = thread::Builder::new()
        .name("plan-run-start".into())
        .spawn(move || start_worker(ctx, guard, &loom_bin, &request, steps))
        .map_err(|error| ApiResponse::error(500, "internal", &error.to_string()))?;
    req.ctx.shutdown().register(ShutdownHook {
        name: "run start",
        drain: Box::new(move || {
            let _ = worker.join();
        }),
        abort: Box::new(|| {}),
    });
    Ok(())
}

fn start_worker(
    ctx: Arc<PlanContext>,
    _guard: JobGuard,
    loom_bin: &Path,
    request: &RunStartRequest,
    steps: Steps,
) {
    let mut step = "loom init";
    let result = sequence(
        ctx.root(),
        loom_bin,
        request,
        steps,
        &mut step,
        &mut |step, stream, line| {
            child::emit_line(&ctx, RunJob::Start, step, stream, line);
        },
    );
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            child::emit_line(
                &ctx,
                RunJob::Start,
                step,
                OutputStream::Stderr,
                &error.to_string(),
            );
            -1
        }
    };
    child::emit_done(&ctx, RunJob::Start, step, code);
}

#[cfg(test)]
fn run_start_sequence(
    root: &Path,
    loom_bin: &Path,
    request: &RunStartRequest,
    steps: Steps,
    on_output: &mut dyn FnMut(&str, OutputStream, &str),
) -> io::Result<i32> {
    sequence(root, loom_bin, request, steps, &mut "loom init", on_output)
}

// Track steps before spawning, including commands that never produce an output line.
fn sequence(
    root: &Path,
    loom_bin: &Path,
    request: &RunStartRequest,
    steps: Steps,
    step: &mut &'static str,
    on_output: &mut dyn FnMut(&str, OutputStream, &str),
) -> io::Result<i32> {
    if steps == Steps::InitThenRun {
        let code = run_init(root, loom_bin, request, step, on_output)?;
        if code != 0 {
            return Ok(code);
        }
    }
    *step = "loom run";
    let mut run = Command::new(loom_bin);
    run.current_dir(root).arg("run");
    run_step(run, step, on_output)
}

fn run_init(
    root: &Path,
    loom_bin: &Path,
    request: &RunStartRequest,
    step: &mut &'static str,
    on_output: &mut dyn FnMut(&str, OutputStream, &str),
) -> io::Result<i32> {
    *step = "loom init";
    let mut init = Command::new(loom_bin);
    init.current_dir(root)
        .arg("init")
        .arg(&request.plan)
        .arg("--backend");
    init.arg(match request.backend {
        Backend::Tmux => "tmux",
        Backend::Native => "native",
    });
    if request.clean {
        init.arg("--clean");
    }
    run_step(init, step, on_output)
}

fn run_step(
    command: Command,
    step: &str,
    on_output: &mut dyn FnMut(&str, OutputStream, &str),
) -> io::Result<i32> {
    match child::run_streamed(command, |stream, line| on_output(step, stream, line)) {
        Ok(status) => Ok(status.code().unwrap_or(-1)),
        Err(error) if error.kind() == ErrorKind::TimedOut => Ok(-1),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_refusal;
