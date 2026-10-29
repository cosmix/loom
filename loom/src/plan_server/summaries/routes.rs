//! The `/api/summaries/...` routes.

use std::sync::Arc;

use serde::Deserialize;

use super::rebuilds::{Rebuilds, ReleaseRefused};
use super::scheduler::{AnswerRefused, AnswerRight};
use crate::plan_server::lifecycle::safe;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::store::{expected_ids, validate_id};

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
struct Ids {
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct Answer {
    question: String,
    rebuild: bool,
}

/// Summary panel reads and rebuild, terminal-claim release and approval actions.
pub fn routes() -> Vec<RouteSpec> {
    vec![
        RouteSpec {
            method: Method::Get,
            path: "/api/summaries/panel",
            body: BodyKind::None,
            role: Role::Viewer,
            handler: panel,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/summaries/rebuild",
            body: BodyKind::Json,
            role: Role::Viewer,
            handler: rebuild,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/summaries/release",
            body: BodyKind::Json,
            role: Role::Viewer,
            handler: release,
        },
        RouteSpec {
            method: Method::Post,
            path: "/api/summaries/answer",
            body: BodyKind::Json,
            role: Role::Viewer,
            handler: answer,
        },
    ]
}

fn state(req: &ApiRequest<'_>) -> Result<Arc<Rebuilds>, ApiResponse> {
    req.ctx
        .extensions()
        .get::<Rebuilds>()
        .ok_or_else(|| ApiResponse::error(500, "internal", "summary rebuilds are unavailable"))
}

fn writable(req: &ApiRequest<'_>) -> Result<(Arc<Rebuilds>, AnswerRight), ApiResponse> {
    let rebuilds = state(req)?;
    if let Some(reason) = rebuilds.panel().read_only {
        return Err(ApiResponse::error(409, "read_only", &safe(&reason)));
    }
    // These actions use the lease's current right without taking authoring.
    let right = if req.ctx.authoring().is_holder(req.browser) {
        AnswerRight::Holder
    } else if req.ctx.authoring().holder().is_none() {
        AnswerRight::FreeAuthoring
    } else {
        AnswerRight::NotAllowed
    };
    if right == AnswerRight::NotAllowed {
        return Err(ApiResponse::error(
            403,
            "not_allowed",
            "another browser holds authoring",
        ));
    }
    Ok((rebuilds, right))
}

/// The body's ids, at most 64, each a well-formed summary id.
fn well_formed_ids(req: &ApiRequest<'_>) -> Result<Vec<String>, ApiResponse> {
    let body: Ids = req.json()?;
    if body.ids.len() > 64 {
        return Err(ApiResponse::error(
            400,
            "too_many_ids",
            "at most 64 summary ids are allowed",
        ));
    }
    if let Some(id) = body.ids.iter().find(|id| validate_id(id).is_err()) {
        return Err(ApiResponse::error(
            400,
            "bad_id",
            &format!("invalid summary id: {}", safe(id)),
        ));
    }
    Ok(body.ids)
}

/// [`well_formed_ids`], each also an expected summary id.
fn ids(req: &ApiRequest<'_>) -> Result<Vec<String>, ApiResponse> {
    let ids = well_formed_ids(req)?;
    let expected = expected_ids(&req.ctx.project());
    if let Some(id) = ids.iter().find(|id| !expected.contains(id)) {
        return Err(ApiResponse::error(
            400,
            "unknown_id",
            &format!("unknown summary id: {}", safe(id)),
        ));
    }
    Ok(ids)
}

fn panel(req: &ApiRequest<'_>) -> ApiResponse {
    match state(req) {
        Ok(rebuilds) => ApiResponse::json(200, &rebuilds.panel()),
        Err(response) => response,
    }
}

fn rebuild(req: &ApiRequest<'_>) -> ApiResponse {
    let (rebuilds, _) = match writable(req) {
        Ok(allowed) => allowed,
        Err(response) => return response,
    };
    let ids = match ids(req) {
        Ok(ids) => ids,
        Err(response) => return response,
    };
    rebuilds.enqueue(ids, Trigger::Button);
    ApiResponse::json(202, &rebuilds.panel())
}

fn release(req: &ApiRequest<'_>) -> ApiResponse {
    let (rebuilds, _) = match writable(req) {
        Ok(allowed) => allowed,
        Err(response) => return response,
    };
    // Whether an id is a terminal claim is the facade's call, not the expected set's: a claim
    // on a summary whose document was removed stays in the panel and must be releasable.
    let ids = match well_formed_ids(req) {
        Ok(ids) => ids,
        Err(response) => return response,
    };
    match rebuilds.release_terminal_claims(&ids) {
        Ok(()) => ApiResponse::json(200, &rebuilds.panel()),
        Err(ReleaseRefused::NotTerminal(id)) => ApiResponse::error(
            409,
            "not_terminal",
            &format!("no terminal claim for {}", safe(&id)),
        ),
        Err(ReleaseRefused::Failed(why)) => ApiResponse::error(500, "internal", &safe(&why)),
    }
}

fn answer(req: &ApiRequest<'_>) -> ApiResponse {
    let (rebuilds, right) = match writable(req) {
        Ok(allowed) => allowed,
        Err(response) => return response,
    };
    let body: Answer = match req.json() {
        Ok(body) => body,
        Err(response) => return response,
    };
    match rebuilds.answer(&body.question, body.rebuild, right) {
        Ok(()) => ApiResponse::json(200, &rebuilds.panel()),
        Err(AnswerRefused::NotAllowed) => {
            ApiResponse::error(403, "not_allowed", "another browser holds authoring")
        }
        Err(AnswerRefused::NoSuchQuestion) => {
            ApiResponse::error(404, "no_such_question", "no such question is open")
        }
    }
}
