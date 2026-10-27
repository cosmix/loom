//! Full-text search over the planning documents.

pub mod index;
pub mod query;

use std::sync::Arc;

use super::context::PlanContext;
use super::files::texts::TextStore;
use super::request::{ApiRequest, ApiResponse};
use super::routes::{BodyKind, Method, Role, RouteSpec};
pub use index::{Scope, SearchIndex};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_query;

pub fn routes() -> Vec<RouteSpec> {
    vec![RouteSpec {
        method: Method::Get,
        path: "/api/search",
        body: BodyKind::None,
        role: Role::Viewer,
        handler: search,
    }]
}

/// Install after file mode so its text store and batch hook are already registered.
pub fn install(ctx: &Arc<PlanContext>) {
    let texts = ctx
        .extensions()
        .get::<TextStore>()
        .expect("file mode must be installed before search");
    let index = Arc::new(SearchIndex::build(&ctx.project(), &texts));
    ctx.extensions().insert(Arc::clone(&index));
    ctx.on_batch(Box::new(move |ctx, batch| {
        index.apply(&ctx.project(), &texts, batch);
    }));
}

fn search(req: &ApiRequest<'_>) -> ApiResponse {
    let query = req.query.get("q").unwrap_or("");
    if query.chars().count() > 256 {
        return ApiResponse::error(400, "bad_request", "search query exceeds 256 characters");
    }
    let scope = match req.query.get("scope").unwrap_or("all") {
        "all" | "" => Scope::All,
        "product" => Scope::Product,
        "plans" => Scope::Plans,
        "knowledge" => Scope::Knowledge,
        _ => return ApiResponse::error(400, "bad_request", "invalid search scope"),
    };
    match req.ctx.extensions().get::<SearchIndex>() {
        Some(index) => ApiResponse::json(200, &index.query(query, scope)),
        None => ApiResponse::error(500, "internal", "search is unavailable"),
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;
    use crate::plan_server::start_with_routes;
    use crate::plan_server::tests::support::{bootstrap, get, options, project, skip, status};

    #[test]
    fn search_route_checks_query_length_and_scope() {
        if skip("search_route_checks_query_length_and_scope") {
            return;
        }
        let project = project();
        let server = start_with_routes(options(&project), routes()).unwrap();
        let texts = TextStore::load(project.path(), &server.ctx.project()).unwrap();
        server.ctx.extensions().insert(Arc::new(texts));
        install(&server.ctx);
        let cookies = bootstrap(&server);
        assert_eq!(
            status(&get(&server, "/api/search?q=demo&scope=product", &cookies)),
            200
        );
        assert_eq!(
            status(&get(&server, "/api/search?scope=invalid", &cookies)),
            400
        );
        let path = format!("/api/search?q={}", "a".repeat(257));
        assert_eq!(status(&get(&server, &path, &cookies)), 400);
        server.stop().unwrap();
    }
}
