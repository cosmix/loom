//! Bulk-settle edge cases beyond the frozen role and confirmation contracts.

use serde_json::json;

use crate::plan_server::tests::support::{bootstrap, json_body, post_json, serve, skip, status};
use crate::planning::project::Project;
use crate::planning::trace::{settle, DocRole};

use super::route_tests::{demo, put, read, SECTION_01};
use super::settle_all::role;

const SECTION_02: &str = "doc/product/demo/section_02.md";
const PLAN: &str = "doc/plans/demo/PLAN-first.md";

#[test]
fn bulk_roles_are_exactly_section_and_plan() {
    assert_eq!(role(Some("section")), Some(DocRole::Section));
    assert_eq!(role(Some("plan")), Some(DocRole::Plan));
    for invalid in [
        None,
        Some(""),
        Some("vision"),
        Some("metaplan"),
        Some("../x"),
    ] {
        assert_eq!(role(invalid), None);
    }
}

#[test]
fn already_settled_documents_are_omitted() {
    if skip("already_settled_documents_are_omitted") {
        return;
    }
    let dir = demo(&[]);
    let project = Project::load(dir.path()).unwrap();
    let doc = project
        .feature("demo")
        .unwrap()
        .trace
        .documents
        .iter()
        .find(|doc| doc.path.to_str() == Some(SECTION_01))
        .unwrap();
    let marked = settle::mark(&read(&dir, SECTION_01), &doc.current);
    put(dir.path(), SECTION_01, &marked);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let response = post_json(
        &server,
        "/api/settle-all?feature=demo&role=section",
        &cookies,
        "",
    );
    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({ "settled": [SECTION_02], "skipped": [] }),
    );
    assert_eq!(read(&dir, SECTION_01), marked);
    server.stop().unwrap();
}

#[test]
fn plans_leave_sections_metaplan_and_stubs_untouched_even_with_clear() {
    if skip("plans_leave_sections_metaplan_and_stubs_untouched_even_with_clear") {
        return;
    }
    let dir = demo(&[]);
    let sections = [read(&dir, SECTION_01), read(&dir, SECTION_02)];
    let metaplan = read(&dir, "doc/plans/demo/index.md");
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let response = post_json(
        &server,
        "/api/settle-all?feature=demo&role=plan&clear=1",
        &cookies,
        "",
    );
    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({ "settled": [PLAN], "skipped": [] }),
    );
    assert!(read(&dir, PLAN)
        .lines()
        .any(|line| line.starts_with("Settled: ")));
    assert_eq!([read(&dir, SECTION_01), read(&dir, SECTION_02)], sections);
    assert_eq!(read(&dir, "doc/plans/demo/index.md"), metaplan);
    assert!(!dir.path().join("doc/plans/demo/PLAN-second.md").exists());
    server.stop().unwrap();
}

#[test]
fn a_stale_first_document_does_not_stop_the_second() {
    if skip("a_stale_first_document_does_not_stop_the_second") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let path = dir.path().join(SECTION_01);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let edited = read(&dir, SECTION_01).replace("Text one.", "Text uno.");
    std::fs::write(&path, &edited).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let response = post_json(
        &server,
        "/api/settle-all?feature=demo&role=section&confirm=1",
        &cookies,
        "",
    );
    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({
            "settled": [SECTION_02],
            "skipped": [{ "path": SECTION_01, "reason": "stale" }],
        }),
    );
    assert_eq!(read(&dir, SECTION_01), edited);
    assert!(read(&dir, SECTION_02)
        .lines()
        .any(|line| line.starts_with("Settled: ")));
    server.stop().unwrap();
}

#[test]
fn unknown_feature_and_missing_role_are_bad_requests() {
    if skip("unknown_feature_and_missing_role_are_bad_requests") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    for query in [
        "feature=absent&role=section",
        "feature=&role=plan",
        "feature=demo",
    ] {
        let response = post_json(&server, &format!("/api/settle-all?{query}"), &cookies, "");
        assert_eq!(status(&response), 400, "{response}");
        assert_eq!(json_body(&response)["error"], "bad_request");
    }
    server.stop().unwrap();
}
