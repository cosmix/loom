//! Contract: `GET /api/session/commands` is a Viewer route of `all_routes()` that answers a
//! session it does not know with the Loading view of the native rows, never with 404.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::request::{ApiRequest, ApiResponse, QueryParams};
use crate::plan_server::routes::{all_routes, Method, Role, RouteSpec};
use crate::planning::watch::OwnWrites;
use crate::web_server::http::parse_head;

const PATH: &str = "/api/session/commands";

/// `LOOM_HOME` pointed at a temporary home, as the guidance tests do, restored on drop.
struct HomeGuard(Option<OsString>);

impl HomeGuard {
    fn set(home: &Path) -> Self {
        let previous = std::env::var_os("LOOM_HOME");
        std::env::set_var("LOOM_HOME", home);
        Self(previous)
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.0 {
            Some(value) => std::env::set_var("LOOM_HOME", value),
            None => std::env::remove_var("LOOM_HOME"),
        }
    }
}

fn commands_route() -> RouteSpec {
    let routes = all_routes();
    let route = routes.into_iter().find(|route| route.path == PATH);
    let route = route.expect("all_routes() holds /api/session/commands");
    assert_eq!((route.method, route.role), (Method::Get, Role::Viewer));
    route
}

fn call(route: &RouteSpec, ctx: &PlanContext, query: &str) -> ApiResponse {
    let raw = format!("GET {PATH}{query} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let head = parse_head(raw.as_bytes()).unwrap().unwrap();
    let browser = BrowserId::parse(&"ab".repeat(16)).unwrap();
    let public_id = PublicId::mint();
    let request = ApiRequest {
        ctx,
        head: &head,
        browser: &browser,
        public_id: &public_id,
        query: QueryParams::parse(head.query.as_deref()).unwrap(),
        body: Vec::new(),
    };
    (route.handler)(&request)
}

fn assert_loading_native_rows(response: &ApiResponse, query: &str) {
    assert_eq!(response.status, 200, "{query}");
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["state"], "loading", "{query}: {body}");
    assert_eq!(body["generation"], Value::Null, "{query}: {body}");
    let entries = body["entries"].as_array().expect("an entries list");
    let clear = entries.iter().find(|entry| entry["name"] == "clear");
    let clear = clear.unwrap_or_else(|| panic!("{query}: no clear row: {body}"));
    assert_eq!(clear["source"], "built-in", "{clear}");
    assert_eq!(clear["class"], "terminal-only", "{clear}");
    let bundled = entries.iter().find(|entry| entry["source"] != "built-in");
    assert_eq!(bundled, None, "{query}: a non-native row while loading");
}

#[test]
#[serial_test::serial]
fn the_commands_route_answers_loading_for_an_unknown_session() {
    let temp = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(&temp.path().join("home"));
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("doc/plans")).unwrap();
    let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(root, read_only, bind, OwnWrites::default()).unwrap();

    let route = commands_route();
    for query in ["?session=nope", ""] {
        assert_loading_native_rows(&call(&route, &ctx, query), query);
    }
}
