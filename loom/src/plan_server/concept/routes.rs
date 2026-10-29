//! Concept routes: `GET /api/concept/<page>`.

use crate::plan_server::context::PlanContext;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_server::summaries::rebuilds::Rebuilds;

use super::engine::ConceptState;
use super::page::{load_page, PageContext, PageKey};

const PREFIX: &str = "/api/concept/";

pub fn routes() -> Vec<RouteSpec> {
    vec![RouteSpec {
        method: Method::Get,
        path: PREFIX,
        body: BodyKind::None,
        role: Role::Viewer,
        handler: page,
    }]
}

/// Server state a concept page needs: read-only problems and summary rebuilds.
#[allow(clippy::needless_update)]
pub(crate) fn page_context(ctx: &PlanContext) -> PageContext {
    PageContext {
        read_only: ctx
            .read_only()
            .problems()
            .first()
            .map(|problem| problem.message.clone()),
        rebuilds: ctx.extensions().get::<Rebuilds>(),
        ..Default::default()
    }
}

fn page(req: &ApiRequest<'_>) -> ApiResponse {
    let rest = req.head.path.strip_prefix(PREFIX).unwrap_or_default();
    let Some(key) = PageKey::parse(rest) else {
        return ApiResponse::error(400, "bad_page", "not a concept page key");
    };
    let Some(state) = req.ctx.extensions().get::<ConceptState>() else {
        return ApiResponse::error(500, "internal", "concept mode is unavailable");
    };
    let eval = state.latest();
    let context = page_context(req.ctx);
    let project = req.ctx.project();
    match load_page(&project, &eval, &key, &context) {
        Some(page) => ApiResponse::json(200, &page),
        None => ApiResponse::error(404, "not_found", "no such concept page"),
    }
}
