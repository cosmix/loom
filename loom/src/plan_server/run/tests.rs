//! Run handoff contracts exercised through the installed route table and shared fixtures.

use std::process::Command;

use crate::fs::work_dir::WorkDir;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::routes::{all_routes, find, BodyKind, Lookup, Method, Role};
use crate::plan_server::tests::support as http;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::run::RunJob;

#[test]
fn route_table_holds_each_method_and_path_once() {
    let routes = all_routes();

    for (index, route) in routes.iter().enumerate() {
        assert!(
            !routes[..index]
                .iter()
                .any(|prior| prior.method == route.method && prior.path == route.path),
            "duplicate route: {:?} {}",
            route.method,
            route.path
        );
    }
}

#[test]
fn run_and_pressure_routes_have_the_required_roles() {
    use Method::{Get, Post};
    use Role::{Holder, HolderOrTake, Viewer};
    let expected = [
        (Get, "/api/run/state", Viewer),
        (Post, "/api/run/commit", Holder),
        (Post, "/api/run/start", Holder),
        (Post, "/api/run/dashboard", Viewer),
        (Post, "/api/run/abandon", Holder),
        (Post, "/api/run/finish-abandon", Holder),
        (Get, "/api/pressure/options", Viewer),
        (Post, "/api/pressure/start", HolderOrTake),
        (Post, "/api/pressure/stop", Holder),
        (Post, "/api/pressure/confirm-config", HolderOrTake),
    ];
    let routes = all_routes();

    for (method, path, role) in expected {
        let verb = if method == Get { "GET" } else { "POST" };
        let Lookup::Found(route) = find(&routes, verb, path) else {
            panic!("missing route: {verb} {path}");
        };
        assert_eq!((route.method, route.role), (method, role), "{path}");
    }
    assert_eq!(
        routes
            .iter()
            .filter(|route| {
                route.path.starts_with("/api/run/") || route.path.starts_with("/api/pressure/")
            })
            .count(),
        expected.len()
    );
}

#[test]
fn run_routes_accept_the_required_body_kinds() {
    let routes = all_routes();
    let expected = [
        ("GET", "/api/run/state", BodyKind::None),
        ("POST", "/api/run/commit", BodyKind::Json),
        ("POST", "/api/run/start", BodyKind::Json),
        ("POST", "/api/run/dashboard", BodyKind::None),
    ];

    for (method, path, body) in expected {
        let Lookup::Found(route) = find(&routes, method, path) else {
            panic!("missing route: {method} {path}");
        };
        assert_eq!(route.body, body, "{path}");
    }
}

fn init_repo(dir: &tempfile::TempDir) {
    let output = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(dir.path())
        .output()
        .expect("initialize the scratch repository");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn viewer_run_state_contains_no_dashboard_token() {
    if http::skip("viewer_run_state_contains_no_dashboard_token") {
        return;
    }
    let dir = http::project();
    init_repo(&dir);
    let server = http::serve(&dir);
    let cookie = http::bootstrap(&server);

    let response = http::get(&server, "/api/run/state", &cookie);

    assert_eq!(http::status(&response), 200, "{response}");
    assert!(!http::body(&response).contains("token="), "{response}");
    let drains = server.ctx.shutdown().begin(|_| {});
    server.stop_when_drained(drains).expect("stop server");
}

#[test]
fn every_broadcast_run_strip_contains_no_dashboard_token() {
    if http::skip("every_broadcast_run_strip_contains_no_dashboard_token") {
        return;
    }
    let dir = http::project();
    let work = WorkDir::new(dir.path()).expect("work directory");
    work.initialize().expect("initialize scratch workspace");
    let server = http::serve(&dir);
    let conn = server.ctx.conn_ctx(BrowserId::mint().expect("viewer"));
    let frames = server.ctx.hub().register(&conn);
    let urls = [
        "http://127.0.0.1:7373/?token=dashboard-secret",
        "http://127.0.0.1:7373/?tab=stages&token=dashboard-secret&view=compact",
        "http://127.0.0.1:7373/?token=dashboard-secret&token=second-secret",
    ];

    for url in urls {
        let strip = super::strip::build_strip(&work, Some(url)).expect("strip snapshot");
        assert!(
            strip.dashboard_url.is_some(),
            "exercise a populated dashboard URL"
        );
        server.ctx.hub().broadcast(&PlanMessage::RunStrip(strip));
    }

    let mut strips = 0;
    for frame in frames.try_iter() {
        let message: PlanMessage = serde_json::from_str(&frame).expect("wire frame");
        if matches!(message, PlanMessage::RunStrip(_)) {
            strips += 1;
            assert!(!frame.contains("token="), "{frame}");
            assert!(!frame.contains("dashboard-secret"), "{frame}");
            assert!(!frame.contains("second-secret"), "{frame}");
        }
    }
    assert!(strips >= urls.len(), "all seeded strips reached the viewer");
    let drains = server.ctx.shutdown().begin(|_| {});
    server.stop_when_drained(drains).expect("stop server");
}

#[test]
fn commit_while_another_job_runs_returns_busy() {
    if http::skip("commit_while_another_job_runs_returns_busy") {
        return;
    }
    let dir = http::project();
    init_repo(&dir);
    let server = http::serve(&dir);
    let cookie = http::bootstrap(&server);
    let origin = http::origin(&server);
    let take = http::request("POST", &server, "/api/test/take", &cookie, &[&origin], "");
    assert_eq!(
        http::status(&http::send(server.local, take.as_bytes())),
        200
    );
    let jobs = super::child::jobs(&server.ctx);
    let guard = jobs.begin(RunJob::Start).expect("reserve the running job");

    let response = http::post_json(
        &server,
        "/api/run/commit",
        &cookie,
        r#"{"paths":["doc/product/index.md"],"message":"Save docs"}"#,
    );

    assert_eq!(http::status(&response), 409, "{response}");
    assert_eq!(http::json_body(&response)["error"], "busy");
    drop(guard);
    let drains = server.ctx.shutdown().begin(|_| {});
    server.stop_when_drained(drains).expect("stop server");
}
