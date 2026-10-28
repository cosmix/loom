//! Read-only access to the guidance rendered for a session.

use serde::{Deserialize, Serialize};

use super::provider::PlanGuidance;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};

/// The rendered brief and launch settings shown by the session's Brief link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBriefView {
    pub session_id: String,
    pub label: String,
    pub brief: String,
    pub model: String,
    pub effort: String,
    pub display_name: String,
}

pub fn route() -> RouteSpec {
    RouteSpec {
        method: Method::Get,
        path: "/api/session/brief",
        body: BodyKind::None,
        role: Role::Viewer,
        handler: brief,
    }
}

fn brief(req: &ApiRequest<'_>) -> ApiResponse {
    let Some(session_id) = req.query.get("session") else {
        return ApiResponse::error(404, "unknown_session", "unknown session");
    };
    let Some(guidance) = req.ctx.extensions().get::<PlanGuidance>() else {
        return ApiResponse::error(500, "internal", "session guidance is unavailable");
    };
    // Session ids are exact map keys, never filesystem paths.
    match guidance.view(session_id) {
        Some(view) => ApiResponse::json(200, &view),
        None => ApiResponse::error(404, "unknown_session", "unknown session"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn session_brief_fixture_matches_serde_output() {
        let fixture = include_str!("../../../../web/src/plan/api/fixtures/session-brief.json");
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let model: SessionBriefView = serde_json::from_value(expected.clone()).unwrap();

        assert_eq!(serde_json::to_value(&model).unwrap(), expected);
        assert_eq!(
            format!("{}\n", serde_json::to_string_pretty(&model).unwrap()),
            fixture
        );
    }
}
