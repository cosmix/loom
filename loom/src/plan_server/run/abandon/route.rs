//! `POST /api/run/abandon` and `POST /api/run/finish-abandon`.
//!
//! Both run on the request's connection thread and answer with the outcome once the job ends:
//! a `BranchesChanged` found after `loom stop` must still reach the dialog as a 409. The output
//! streams meanwhile as `RunOutput` frames.

use serde_json::json;

use crate::plan_server::authoring::Refusal;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_server::run::child::{emit_done, emit_line, jobs};
use crate::plan_server::wire::run::{AbandonRequest, OutputStream, RunJob};

use super::plan::running_plan;
use super::{run_abandon, wire_branches, AbandonOutcome};

pub(super) fn routes() -> Vec<RouteSpec> {
    let holder = |path, handler: fn(&ApiRequest<'_>) -> ApiResponse| RouteSpec {
        method: Method::Post,
        path,
        // The web client always posts a JSON body; finish-abandon ignores it.
        body: BodyKind::Json,
        role: Role::Holder,
        handler,
    };
    vec![
        holder("/api/run/abandon", abandon),
        holder("/api/run/finish-abandon", finish_abandon),
    ]
}

fn abandon(req: &ApiRequest<'_>) -> ApiResponse {
    let ctx = req.ctx;
    if ctx.read_only().is_read_only() {
        return read_only(ctx);
    }
    let request: AbandonRequest = match req.json() {
        Ok(request) => request,
        Err(response) => return response,
    };
    let Some(_job) = jobs(ctx).begin(RunJob::Abandon) else {
        return busy();
    };
    if running_plan(ctx.root()).is_none() {
        let message = "only a running or stopped IN_PROGRESS- plan can be abandoned";
        return ApiResponse::error(409, "not_abandonable", message);
    }
    let loom_bin = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!("abandon: the loom executable cannot be found: {error}");
            return ApiResponse::error(500, "internal", "the loom executable cannot be found");
        }
    };
    let confirmed: Vec<(String, usize)> = request
        .confirmed_unmerged
        .into_iter()
        .map(|b| (b.branch, usize::try_from(b.ahead).unwrap_or(usize::MAX)))
        .collect();
    let mut on_line = |line: &str| {
        emit_line(
            ctx,
            RunJob::Abandon,
            ABANDON_STEP,
            OutputStream::Stdout,
            line,
        )
    };
    let outcome = run_abandon(
        ctx.root(),
        ctx.runtime(),
        &loom_bin,
        &confirmed,
        &mut on_line,
    );
    emit_done_for(ctx, RunJob::Abandon, &outcome);
    respond(outcome)
}

/// The step label of the abandon sequence's output lines.
const ABANDON_STEP: &str = "loom abandon";

/// The job's one `done` frame, from how it ended.
fn emit_done_for(ctx: &PlanContext, job: RunJob, outcome: &anyhow::Result<AbandonOutcome>) {
    let (step, code) = match outcome {
        Ok(AbandonOutcome::Renamed { .. }) => ("rename", 0),
        Ok(AbandonOutcome::Refused { .. }) => ("verify", 1),
        Ok(AbandonOutcome::BranchesChanged { .. }) => ("branches", 1),
        Ok(AbandonOutcome::Failed { step, code }) => (step.as_str(), *code),
        Err(_) => (ABANDON_STEP, -1),
    };
    emit_done(ctx, job, step, code);
}

fn finish_abandon(req: &ApiRequest<'_>) -> ApiResponse {
    let ctx = req.ctx;
    if ctx.read_only().is_read_only() {
        return read_only(ctx);
    }
    let Some(_job) = jobs(ctx).begin(RunJob::FinishAbandon) else {
        return busy();
    };
    let outcome = super::finish_abandon(ctx.root(), ctx.runtime());
    emit_done_for(ctx, RunJob::FinishAbandon, &outcome);
    respond(outcome)
}

fn read_only(ctx: &PlanContext) -> ApiResponse {
    ApiResponse::error(
        403,
        "read_only",
        &ctx.refusal_message(&Refusal::ReadOnlyMode),
    )
}

fn busy() -> ApiResponse {
    ApiResponse::error(409, "busy", "another run job is in progress")
}

fn respond(result: anyhow::Result<AbandonOutcome>) -> ApiResponse {
    match result {
        Ok(AbandonOutcome::Renamed { to }) => {
            ApiResponse::json(200, &json!({ "to": to.to_string_lossy() }))
        }
        Ok(AbandonOutcome::Refused { reason }) => ApiResponse::error(409, "refused", &reason),
        Ok(AbandonOutcome::Failed { step, code }) => {
            let message = format!("{step} exited with code {code}; nothing after it ran");
            ApiResponse::error(409, "step_failed", &message)
        }
        Ok(AbandonOutcome::BranchesChanged { now }) => branches_changed(now),
        Err(error) => {
            tracing::warn!("abandon: {error:#}");
            ApiResponse::error(500, "internal", "the abandon could not complete")
        }
    }
}

/// 409 `unmerged_branches` carrying the fresh list for the second confirmation.
fn branches_changed(now: Vec<(String, usize)>) -> ApiResponse {
    let body = json!({
        "error": "unmerged_branches",
        "message": "the loom/* branches with unmerged commits changed; confirm the new list",
        "unmerged_branches": wire_branches(now),
    });
    ApiResponse::json(409, &body)
}
