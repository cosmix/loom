//! The pressure routes: the dialog's options, start, stop and the confirmation of a Codex
//! configuration change.

use serde_json::json;

use super::driver::{accept, announce_options_changed, StartRefused};
use super::install::PressureRuns;
use super::options::{config_block, options, validate_start};
use super::view::StopCause;
use crate::plan_server::authoring::Refusal;
use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_server::wire::pressure::{disabled_reason, PressureRunView, PressureStart};
use crate::planning::guidance::StartBlocks;
use crate::pressure::codex_run::codex_skill_installed;

pub fn all() -> Vec<RouteSpec> {
    vec![
        RouteSpec {
            method: Method::Get,
            path: "/api/pressure/options",
            body: BodyKind::None,
            role: Role::Viewer,
            handler: options_route,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/pressure/start",
            body: BodyKind::Json,
            role: Role::HolderOrTake,
            handler: start_route,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/pressure/stop",
            body: BodyKind::None,
            role: Role::Holder,
            handler: stop_route,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/pressure/confirm-config",
            body: BodyKind::None,
            // Authoring lives in memory: after a restart nobody holds it, and a persisted block
            // must still be confirmable by the browser that takes it.
            role: Role::HolderOrTake,
            handler: confirm_route,
        },
    ]
}

/// The dialog's options; a configuration block makes them unavailable with its reason.
fn options_route(req: &ApiRequest<'_>) -> ApiResponse {
    let mut offered = options(req.ctx, req.browser);
    if offered.available {
        if let Some(block) = &offered.config_block {
            offered.available = false;
            offered.disabled_reason = Some(block.reason.clone());
        }
    }
    ApiResponse::json(200, &offered)
}

fn start_route(req: &ApiRequest<'_>) -> ApiResponse {
    match start(req) {
        Ok(view) => ApiResponse::json(202, &view),
        Err(refused) => refused,
    }
}

/// The refusals in their published order, then the accepted run on its own thread.
fn start(req: &ApiRequest<'_>) -> Result<PressureRunView, ApiResponse> {
    let runs = runs(req.ctx)?;
    let start: PressureStart = req.json()?;
    let codex_path = crate::codex::find_codex_path()
        .map_err(|_| refuse(409, "codex_missing", disabled_reason::CODEX_MISSING))?;
    if !codex_skill_installed() {
        let reason = disabled_reason::CODEX_SKILL_MISSING;
        return Err(refuse(409, "codex_skill_missing", reason));
    }
    if req.ctx.read_only().is_read_only() {
        return Err(refuse(409, "read_only", disabled_reason::READ_ONLY));
    }
    if runs.running() {
        return Err(refuse(409, "pressure_running", disabled_reason::RUNNING));
    }
    let checked = validate_start(&req.ctx.project(), &start);
    checked.map_err(|code| invalid(req.ctx, &code))?;
    let accepted = accept(&runs, req.browser, req.public_id, start, codex_path);
    accepted.map_err(|refused| start_refused(req.ctx, refused))
}

fn invalid(ctx: &PlanContext, code: &str) -> ApiResponse {
    match code {
        "invalid_model" => refuse(400, code, "A model or effort is not in its list."),
        "invalid_rounds" => refuse(400, code, "Rounds must be between 1 and 10."),
        "config_blocked" => {
            let reason = config_block(ctx.runtime()).map(|block| block.reason);
            let reason = reason.unwrap_or_else(|| "Codex changed session configuration".into());
            refuse(
                409,
                code,
                &format!("Starts are blocked until it is confirmed: {reason}"),
            )
        }
        _ => refuse(409, code, PLAN_NOT_WRITTEN),
    }
}

const PLAN_NOT_WRITTEN: &str =
    "Only a written plan (doc/plans/PLAN-<slug>.md) can be pressure-tested.";

fn start_refused(ctx: &PlanContext, refused: StartRefused) -> ApiResponse {
    match refused {
        StartRefused::PlanNotWritten => refuse(409, "plan_not_written", PLAN_NOT_WRITTEN),
        StartRefused::Running => refuse(409, "pressure_running", disabled_reason::RUNNING),
        StartRefused::Authoring(refusal) => {
            let code = match refusal {
                Refusal::NotHolder { .. } => "not_holder",
                Refusal::NoHolder => "no_holder",
                Refusal::ReadOnlyMode => "read_only",
                Refusal::Stopping => "stopping",
            };
            refuse(403, code, &ctx.refusal_message(&refusal))
        }
        StartRefused::Unavailable(why) => refuse(503, "unavailable", &why),
    }
}

/// Stop the whole run, named after the holder who pressed it.
fn stop_route(req: &ApiRequest<'_>) -> ApiResponse {
    let runs = match runs(req.ctx) {
        Ok(runs) => runs,
        Err(response) => return response,
    };
    let name = req.ctx.presence().name_of(req.browser);
    let name = if name.is_empty() {
        "the holder".to_owned()
    } else {
        name
    };
    match runs.stop(StopCause::Holder(name)) {
        Some(_) => ApiResponse::json(202, &json!({ "stopping": true })),
        None => refuse(409, "no_run", "No pressure test is running."),
    }
}

/// The holder (or the browser that took authoring while it was free) checked the files Codex
/// changed: launches may start again. Every tab hears it
/// as an options-changed frame, its cue to fetch the options (no block) anew. The frame does
/// not depend on a run view, which a restarted server does not have.
fn confirm_route(req: &ApiRequest<'_>) -> ApiResponse {
    let runtime = req.ctx.runtime();
    if matches!(StartBlocks::current(runtime), Ok(None)) {
        return refuse(
            409,
            "no_block",
            "No configuration change waits for confirmation.",
        );
    }
    if let Err(error) = StartBlocks::clear(runtime) {
        return ApiResponse::error(500, "internal", &format!("{error:#}"));
    }
    announce_options_changed(req.ctx);
    ApiResponse::json(200, &json!({ "cleared": true }))
}

fn runs(ctx: &PlanContext) -> Result<std::sync::Arc<PressureRuns>, ApiResponse> {
    let runs = ctx.extensions().get::<PressureRuns>();
    runs.ok_or_else(|| refuse(503, "unavailable", "Pressure runs are unavailable."))
}

fn refuse(status: u16, code: &str, message: &str) -> ApiResponse {
    ApiResponse::error(status, code, message)
}
