//! The viewer-safe feature index for the planning home screen.

use serde::Serialize;

use super::request::{ApiRequest, ApiResponse};
use super::routes::{BodyKind, Method, Role, RouteSpec};
use super::wire::checklist::{ChecklistView, OfferView, ProjectChecklistView, StandalonePlanView};
use crate::planning::kinds::vision_path;
use crate::planning::project::Project;

/// Register the feature index with the same gate as the other viewer GET routes.
pub fn routes() -> Vec<RouteSpec> {
    vec![RouteSpec {
        method: Method::Get,
        path: "/api/features",
        body: BodyKind::None,
        role: Role::Viewer,
        handler: features,
    }]
}

#[derive(Serialize)]
struct FeaturesView {
    features: Vec<FeatureView>,
    standalone: Vec<StandalonePlanView>,
    moves: Vec<OfferView>,
}

#[derive(Serialize)]
struct FeatureView {
    name: String,
    has_vision: bool,
    has_metaplan: bool,
    checklist: ChecklistView,
}

impl From<&Project> for FeaturesView {
    fn from(project: &Project) -> Self {
        let planning = ProjectChecklistView::from(project);
        let features = planning
            .checklists
            .into_iter()
            .map(|checklist| FeatureView {
                has_vision: project.tree.get(&vision_path(&checklist.feature)).is_some(),
                has_metaplan: project
                    .feature(&checklist.feature)
                    .is_some_and(|feature| !feature.metaplans.is_empty()),
                name: checklist.feature.clone(),
                checklist,
            })
            .collect();
        Self {
            features,
            standalone: planning.standalone,
            moves: planning.moves,
        }
    }
}

fn features(req: &ApiRequest<'_>) -> ApiResponse {
    ApiResponse::json(200, &FeaturesView::from(&*req.ctx.project()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_server::start_with_routes;
    use crate::plan_server::tests::support::{
        body, bootstrap, get, json_body, options, project, request, send, skip, status,
    };
    use crate::plan_server::wire::checklist::CurrentView;
    use serde_json::json;
    use std::path::Path;

    fn put(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn features_route_is_registered_as_a_viewer_get() {
        let routes = crate::plan_server::routes::all_routes();
        let spec = routes
            .iter()
            .find(|spec| spec.path == "/api/features")
            .unwrap();
        assert_eq!(spec.method, Method::Get);
        assert_eq!(spec.role, Role::Viewer);
        assert_eq!(spec.body, BodyKind::None);
    }

    #[test]
    fn features_route_returns_sorted_features_standalone_and_moves() {
        if skip("features_route_returns_sorted_features_standalone_and_moves") {
            return;
        }
        let project = project();
        put(project.path(), "doc/plans/zeta/index.md", "# Engineering\n");
        put(project.path(), "doc/product/alpha/index.md", "# Product\n");
        put(project.path(), "doc/plans/PLAN-fix.md", "# A small fix\n");
        let server = start_with_routes(options(&project), routes()).unwrap();
        let cookies = bootstrap(&server);

        let response = get(&server, "/api/features", &cookies);
        assert_eq!(status(&response), 200);
        let mut value = json_body(&response);
        let modified = value["standalone"][0]
            .as_object_mut()
            .unwrap()
            .remove("last_change")
            .unwrap();
        assert!(modified.as_str().unwrap().ends_with('Z'));
        let features = value["features"].as_array().unwrap();
        assert_eq!(features.len(), 2);
        assert_eq!(features[0]["name"], "alpha");
        assert_eq!(features[0]["has_vision"], true);
        assert_eq!(features[0]["has_metaplan"], false);
        assert_eq!(features[0]["checklist"]["feature"], "alpha");
        assert_eq!(
            features[0]["checklist"]["steps"].as_array().unwrap().len(),
            6
        );
        assert_eq!(features[1]["name"], "zeta");
        assert_eq!(features[1]["has_vision"], false);
        assert_eq!(features[1]["has_metaplan"], true);
        assert_eq!(features[1]["checklist"]["feature"], "zeta");
        assert_standalone_and_moves(&value);
        server.stop().unwrap();
    }

    fn assert_standalone_and_moves(value: &serde_json::Value) {
        assert_eq!(
            value["standalone"],
            json!([{
                "slug": "fix", "path": "doc/plans/PLAN-fix.md",
                "state": "written", "title": "A small fix"
            }])
        );
        assert_eq!(
            value["moves"],
            json!([{
                "id": "move-into-feature:doc/product/index.md", "kind": "move_into_feature",
                "label": "Move doc/product/index.md into a feature",
                "target": "doc/product/index.md"
            }])
        );
    }

    #[test]
    fn features_route_requires_access_cookie_and_allows_read_only_viewers_and_head() {
        if skip("features_route_requires_access_cookie_and_allows_read_only_viewers_and_head") {
            return;
        }
        let project = project();
        let mut opts = options(&project);
        opts.read_only = crate::plan_server::prereq::ReadOnly::from_problems(vec![
            crate::plan_server::wire::server::MissingPrereq {
                what: crate::plan_server::wire::server::PrereqKind::Tmux,
                message: "missing tmux".into(),
                fix: "install tmux".into(),
            },
        ]);
        let server = start_with_routes(opts, routes()).unwrap();
        assert_eq!(status(&get(&server, "/api/features", "")), 401);
        let cookies = bootstrap(&server);
        assert_eq!(status(&get(&server, "/api/features", &cookies)), 200);
        let head = send(
            server.local,
            request("HEAD", &server, "/api/features", &cookies, &[], "").as_bytes(),
        );
        assert_eq!(status(&head), 200);
        assert_eq!(body(&head), "");
        server.stop().unwrap();
    }

    #[test]
    fn finished_feature_uses_its_slug_and_has_no_primary_offer() {
        let root = tempfile::tempdir().unwrap();
        put(
            root.path(),
            "doc/product/demo/index.md",
            "# Demo\n\nA demo.\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n| 01 | Alpha |\n",
        );
        put(
            root.path(),
            "doc/product/demo/section_01.md",
            "# Alpha\n\nBody.\n",
        );
        put(
            root.path(),
            "doc/plans/demo/index.md",
            "# Engineering\n\n## 1. Done (`PLAN-fix.md`)\n",
        );
        put(
            root.path(),
            "doc/plans/demo/archive/DONE-PLAN-fix.md",
            "# Finished\n",
        );
        let project = Project::load(root.path()).unwrap();
        let view = FeaturesView::from(&project);
        let feature = &view.features[0];

        assert_eq!(feature.name, "demo");
        assert_eq!(feature.checklist.feature, "demo");
        assert_eq!(feature.checklist.current, CurrentView::ReadyAllDone);
        assert_eq!(feature.checklist.primary, None);
        let secondary: Vec<_> = feature
            .checklist
            .secondary
            .iter()
            .map(|offer| (offer.id.as_str(), offer.label.as_str()))
            .collect();
        assert_eq!(
            secondary,
            [
                ("plan-follow-ups", "Plan follow-ups"),
                ("review-feature", "Review the feature"),
            ]
        );
    }

    #[test]
    fn empty_project_has_no_implicit_feature_or_checklist() {
        let root = tempfile::tempdir().unwrap();
        let project = Project::load(root.path()).unwrap();
        assert_eq!(
            serde_json::to_value(FeaturesView::from(&project)).unwrap(),
            json!({
                "features": [], "standalone": [], "moves": []
            })
        );
    }
}
