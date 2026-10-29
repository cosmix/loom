//! Summary routes over the facade `install` made (the real spawner and the context's read-only
//! mode), which `tests::serve` replaces.

use serde_json::json;

use super::tests::{start, PANEL, REBUILD};
use crate::plan_server::lifecycle::safe;
use crate::plan_server::summaries::scheduler::SessionSpawner;
use crate::plan_server::summaries::spawner::RealSpawner;
use crate::plan_server::tests::support::{bootstrap, get, json_body, post_json, skip, status};
use crate::planning::summary::evaluate::fixtures::{self, ID1};

const MESSAGE: &str = "tmux is unavailable";

#[test]
fn the_installed_facade_reports_the_contexts_read_only_message_and_refuses_rebuilds() {
    if skip("the_installed_facade_reports_the_contexts_read_only_message_and_refuses_rebuilds") {
        return;
    }
    let fixture = fixtures::two_sections();
    let server = start(&fixture, Some(MESSAGE));
    let cookies = bootstrap(&server);

    let response = get(&server, PANEL, &cookies);
    assert_eq!(status(&response), 200, "{}", safe(&response));
    assert_eq!(json_body(&response)["read_only"], MESSAGE);

    let body = json!({"ids": [ID1]}).to_string();
    let response = post_json(&server, REBUILD, &cookies, &body);
    assert_eq!(status(&response), 409, "{}", safe(&response));
    assert_eq!(json_body(&response)["error"], "read_only");
    assert!(json_body(&get(&server, PANEL, &cookies))["running"].is_null());
}

#[test]
fn a_writable_context_reports_no_read_only_message() {
    if skip("a_writable_context_reports_no_read_only_message") {
        return;
    }
    let fixture = fixtures::two_sections();
    let server = start(&fixture, None);
    let cookies = bootstrap(&server);

    let response = get(&server, PANEL, &cookies);
    assert_eq!(status(&response), 200, "{}", safe(&response));
    assert!(json_body(&response)["read_only"].is_null());
}

#[test]
fn the_real_spawner_starts_no_session_once_the_server_is_stopping() {
    if skip("the_real_spawner_starts_no_session_once_the_server_is_stopping") {
        return;
    }
    let fixture = fixtures::two_sections();
    let server = start(&fixture, None);
    server.ctx.shutdown().abort_all();

    let error = RealSpawner::new(&server.ctx)
        .start_summary(&[ID1.to_owned()])
        .expect_err("a stopping server starts no summary session");

    assert!(error.to_string().contains("stopping"), "{error:#}");
}
