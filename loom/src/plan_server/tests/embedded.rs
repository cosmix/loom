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
fn embedded_font_is_served_without_a_cookie() {
    if skip("embedded_font_is_served_without_a_cookie") {
        return;
    }
    let project = project();
    let server = start(options(&project)).expect("start the planning server");

    let response = get(
        &server,
        "/fonts/commit-mono-cosmix/commit-mono-cosmix-400-regular.woff2",
        "",
    );

    assert_eq!(status(&response), 200);
    assert!(response.contains("\r\nContent-Type: font/woff2\r\n"));
    assert!(body(&response).starts_with("wOF2"));
    server.stop().expect("stop");
}

#[test]
fn plan_favicon_is_served_as_svg() {
    if skip("plan_favicon_is_served_as_svg") {
        return;
    }
    let project = project();
    let server = start(options(&project)).expect("start the planning server");

    let response = get(&server, "/favicon-plan.svg", "");

    assert_eq!(status(&response), 200);
    assert!(response.contains("\r\nContent-Type: image/svg+xml\r\n"));
    assert!(body(&response).contains("<svg"));
    server.stop().expect("stop");
}

#[test]
fn missing_font_returns_plain_not_found() {
    if skip("missing_font_returns_plain_not_found") {
        return;
    }
    let project = project();
    let server = start(options(&project)).expect("start the planning server");

    let response = get(&server, "/fonts/missing.woff2", "");

    assert_eq!(status(&response), 404, "{response}");
    assert!(response.contains("\r\nContent-Type: text/plain; charset=utf-8\r\n"));
    assert_eq!(body(&response), "not found");
    server.stop().expect("stop");
}

#[test]
fn dashboard_root_still_resolves_to_index_html() {
    let index = assets::page("index.html").expect("embedded index.html");
    let (root, mime) = assets::lookup("/").expect("embedded dashboard root");
    assert_eq!(root, index);
    assert_eq!(mime, "text/html; charset=utf-8");
}
