//! Contracts for `POST /api/settle-all?feature=<f>&role=section|plan[&confirm=1]`: every
//! settleable, unsettled document of the named role is settled through the per-document
//! checks, a refusal is listed under `skipped` and never stops the rest, and any other role is
//! a bad request.

use serde_json::Value;

use crate::plan_server::tests::support::{bootstrap, json_body, post_json, serve, skip, status};

use super::route_tests::{demo, read, SECTION_01};

const SECTION_02: &str = "doc/product/demo/section_02.md";
const PLAN: &str = "doc/plans/demo/PLAN-first.md";
const METAPLAN: &str = "doc/plans/demo/index.md";

fn settled_line(text: &str) -> bool {
    text.lines().any(|line| line.starts_with("Settled: "))
}

/// The repository-relative paths in `value[key]`, read from strings or `{"path": ...}`
/// objects; a path relative to `doc/` gains the prefix.
fn paths(value: &Value, key: &str) -> Vec<String> {
    let items = value[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key}: {value}"));
    items
        .iter()
        .map(|item| item.as_str().or_else(|| item["path"].as_str()).unwrap())
        .map(|path| match path.starts_with("doc/") {
            true => path.to_owned(),
            false => format!("doc/{path}"),
        })
        .collect()
}

#[test]
fn settle_all_settles_only_the_named_role() {
    if skip("settle_all_settles_only_the_named_role") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let response = post_json(
        &server,
        "/api/settle-all?feature=demo&role=section",
        &cookies,
        "",
    );
    assert_eq!(status(&response), 200, "{response}");
    let body = json_body(&response);
    assert_eq!(
        paths(&body, "settled"),
        [SECTION_01, SECTION_02],
        "{response}"
    );
    assert_eq!(paths(&body, "skipped"), Vec::<String>::new(), "{response}");
    for rel in [SECTION_01, SECTION_02] {
        let text = read(&dir, rel);
        assert!(settled_line(&text), "{rel} is settled:\n{text}");
    }
    for rel in [PLAN, METAPLAN] {
        let text = read(&dir, rel);
        assert!(!settled_line(&text), "{rel} is not a section:\n{text}");
    }

    for target in [
        "/api/settle-all?feature=demo&role=../x",
        "/api/settle-all?feature=demo&role=metaplan",
        "/api/settle-all?role=section",
    ] {
        let refused = post_json(&server, target, &cookies, "");
        assert_eq!(status(&refused), 400, "{target}: {refused}");
        assert_eq!(
            json_body(&refused)["error"],
            "bad_request",
            "{target}: {refused}"
        );
    }
    assert!(!settled_line(&read(&dir, PLAN)));
    server.stop().unwrap();
}

#[test]
fn settle_all_skips_unresolved_without_confirm() {
    if skip("settle_all_skips_unresolved_without_confirm") {
        return;
    }
    let open = "# Beta\n\n## Three\n\nText.\n\n## Open questions\n\n- Which store holds it?\n";
    let dir = demo(&[(SECTION_02, open)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let first = post_json(
        &server,
        "/api/settle-all?feature=demo&role=section",
        &cookies,
        "",
    );
    assert_eq!(status(&first), 200, "{first}");
    let body = json_body(&first);
    assert_eq!(paths(&body, "settled"), [SECTION_01], "{first}");
    assert_eq!(paths(&body, "skipped"), [SECTION_02], "{first}");
    assert_eq!(body["skipped"][0]["reason"], "unresolved", "{first}");
    assert!(settled_line(&read(&dir, SECTION_01)));
    assert_eq!(read(&dir, SECTION_02), open);

    let target = "/api/settle-all?feature=demo&role=section&confirm=1";
    let confirmed = post_json(&server, target, &cookies, "");
    assert_eq!(status(&confirmed), 200, "{confirmed}");
    let settled = paths(&json_body(&confirmed), "settled");
    assert!(settled.iter().any(|path| path == SECTION_02), "{confirmed}");
    let text = read(&dir, SECTION_02);
    assert!(
        settled_line(&text),
        "confirm settles the open document:\n{text}"
    );
    server.stop().unwrap();
}
