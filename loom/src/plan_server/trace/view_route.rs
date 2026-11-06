//! `GET /api/trace/<feature>`: the feature's `TraceView`.

use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::wire::trace::TraceView;
use crate::planning::kinds::is_feature_name;

/// The route prefix; the feature name follows.
pub const PREFIX: &str = "/api/trace/";

pub fn trace_route(req: &ApiRequest<'_>) -> ApiResponse {
    let name = req.head.path.strip_prefix(PREFIX).unwrap_or_default();
    let view = is_feature_name(name)
        .then(|| {
            let project = req.ctx.project();
            project
                .feature(name)
                .map(|feature| TraceView::from(&feature.trace))
        })
        .flatten();
    match view {
        Some(view) => ApiResponse::json(200, &view),
        None => ApiResponse::error(404, "not_found", "no such feature"),
    }
}
