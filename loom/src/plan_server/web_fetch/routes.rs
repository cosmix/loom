//! `GET /api/web-fetch` for every browser, `POST /api/web-fetch` for the authoring holder.

use serde::Deserialize;

use super::{settings, ReplaceError};
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::plan_session::web_fetch::{FetchMode, FetchPolicy};

const PATH: &str = "/api/web-fetch";

/// A change: the whole policy and the revision of the read it was made from.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    mode: FetchMode,
    domains: Vec<String>,
    revision: u64,
}

pub fn routes() -> Vec<RouteSpec> {
    vec![
        RouteSpec {
            method: Method::Get,
            path: PATH,
            body: BodyKind::None,
            role: Role::Viewer,
            handler: read,
        },
        RouteSpec {
            method: Method::Post,
            path: PATH,
            body: BodyKind::Json,
            // The gate refuses every other browser before the body is read.
            role: Role::Holder,
            handler: replace,
        },
    ]
}

fn read(req: &ApiRequest<'_>) -> ApiResponse {
    ApiResponse::json(200, &settings(req.ctx).view())
}

/// The whole policy at once; an invalid entry refuses the change and names the entry, and a
/// policy changed since the browser read it (another tab, a card's "Always allow") is refused
/// with 409 so the browser reloads it.
fn replace(req: &ApiRequest<'_>) -> ApiResponse {
    let body: Update = match req.json() {
        Ok(body) => body,
        Err(response) => return response,
    };
    let policy = match FetchPolicy::validated(body.mode, &body.domains) {
        Ok(policy) => policy,
        Err(why) => return ApiResponse::error(400, "invalid_domain", &why),
    };
    let settings = settings(req.ctx);
    match settings.replace(policy, body.revision) {
        Ok(()) => ApiResponse::json(200, &settings.view()),
        Err(ReplaceError::Stale) => ApiResponse::error(
            409,
            "stale_policy",
            "the web fetch settings changed since they were loaded",
        ),
        Err(ReplaceError::Save(error)) => {
            // The chain can name the absolute runtime directory; it goes to the terminal only.
            line(&format!(
                "web fetch settings not saved: {}",
                safe(&format!("{error:#}"))
            ));
            ApiResponse::error(500, "internal", "the web fetch settings could not be saved")
        }
    }
}
