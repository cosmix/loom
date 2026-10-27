//! Tests for the planning and dashboard pages in the embedded web bundle.

use super::support::{body, get, options, project, skip, status};
use crate::plan_server::start;
use crate::web_server::assets;

#[test]
fn plan_page_is_embedded_with_its_entry_script() {
    let page = assets::page("plan.html").expect("embedded plan.html");
    let html = std::str::from_utf8(page).expect("plan.html is UTF-8");
    assert!(html.contains("/assets/plan.js"), "{html}");
}

#[test]
fn spa_path_serves_the_plan_page_without_a_cookie() {
    if skip("spa_path_serves_the_plan_page_without_a_cookie") {
        return;
    }
    let project = project();
    let server = start(options(&project)).expect("start the planning server");
    let response = get(&server, "/files/doc/x.md", "");
    assert_eq!(status(&response), 200, "{response}");
    let page = assets::page("plan.html").expect("embedded plan.html");
    assert_eq!(body(&response).as_bytes(), page);
    server.stop().expect("stop");
}

#[test]
fn dashboard_root_still_resolves_to_index_html() {
    let index = assets::page("index.html").expect("embedded index.html");
    let (root, mime) = assets::lookup("/").expect("embedded dashboard root");
    assert_eq!(root, index);
    assert_eq!(mime, "text/html; charset=utf-8");
}
