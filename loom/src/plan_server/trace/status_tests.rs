//! `POST /api/findings/status`: the refusals, a folded and a planned status, and the
//! status rules on their own.

use serde_json::json;

use super::route_tests::{demo, read};
use super::status_route::status_of;
use crate::plan_server::tests::support::{
    bootstrap, get, json_body, post_json, serve, skip, status,
};
use crate::plan_server::wire::trace::{FindingKindView, FindingStatusView};
use crate::plan_server::RunningServer;
use crate::planning::findings::FindingStatus;
use crate::planning::sections::normalized_hash;

const LEDGER: &str = "doc/plans/demo/findings.md";
const TEXT: &str =
    "# Findings\n\n## F-1 Flag\n\nAbout: section_01#one\nStatus: open\n\nRename the flag.\n";

fn view(kind: FindingKindView, detail: Option<&str>) -> FindingStatusView {
    FindingStatusView {
        kind,
        detail: detail.map(str::to_owned),
    }
}

fn post_status(
    server: &RunningServer,
    cookies: &str,
    id: &str,
    hash: &str,
    status: serde_json::Value,
) -> String {
    let body = json!({ "feature": "demo", "id": id, "hash": hash, "status": status });
    post_json(server, "/api/findings/status", cookies, &body.to_string())
}

/// The first metaplan entry's anchor, as the served trace names it.
fn first_entry(server: &RunningServer, cookies: &str) -> String {
    let view = json_body(&get(server, "/api/trace/demo", cookies));
    view["entries"][0]["anchor"]
        .as_str()
        .expect("an entry")
        .to_owned()
}

#[test]
fn refused_statuses_leave_the_ledger_untouched() {
    if skip("refused_statuses_leave_the_ledger_untouched") {
        return;
    }
    let dir = demo(&[(LEDGER, TEXT)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let hash = normalized_hash(TEXT);
    let cases = [
        ("F-9", json!({ "kind": "dismissed", "detail": "no" })),
        (
            "F-1",
            json!({ "kind": "planned", "detail": "the next plan" }),
        ),
        ("F-1", json!({ "kind": "planned", "detail": "PLAN-.md" })),
        (
            "F-1",
            json!({ "kind": "folded", "detail": "metaplan#no-such-entry" }),
        ),
        ("F-1", json!({ "kind": "folded", "detail": null })),
        ("F-1", json!({ "kind": "dismissed", "detail": " \n " })),
        ("F-1", json!({ "kind": "open", "detail": "x" })),
    ];
    for (id, status_body) in cases {
        let response = post_status(&server, &cookies, id, &hash, status_body.clone());
        assert_eq!(status(&response), 400, "{id} {status_body}: {response}");
        assert_eq!(read(&dir, LEDGER), TEXT);
    }
    server.stop().unwrap();
}

#[test]
fn folded_and_planned_statuses_are_written_as_one_line() {
    if skip("folded_and_planned_statuses_are_written_as_one_line") {
        return;
    }
    let dir = demo(&[(LEDGER, TEXT)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let token = format!("metaplan#{}", first_entry(&server, &cookies));
    let folded = post_status(
        &server,
        &cookies,
        "F-1",
        &normalized_hash(TEXT),
        json!({ "kind": "folded", "detail": token }),
    );
    assert_eq!(status(&folded), 200, "{folded}");
    let written = read(&dir, LEDGER);
    assert!(
        written
            .lines()
            .any(|l| l == format!("Status: folded {token}")),
        "{written}"
    );

    let planned = post_status(
        &server,
        &cookies,
        "F-1",
        &normalized_hash(&written),
        json!({ "kind": "planned", "detail": "PLAN-first.md" }),
    );
    assert_eq!(status(&planned), 200, "{planned}");
    assert!(read(&dir, LEDGER)
        .lines()
        .any(|l| l == "Status: planned PLAN-first.md"));
    server.stop().unwrap();
}

#[test]
fn a_stale_ledger_hash_conflicts_and_an_unknown_feature_is_not_found() {
    if skip("a_stale_ledger_hash_conflicts_and_an_unknown_feature_is_not_found") {
        return;
    }
    let dir = demo(&[(LEDGER, TEXT)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let dismissal = json!({ "kind": "dismissed", "detail": "no" });
    let stale = post_status(
        &server,
        &cookies,
        "F-1",
        &normalized_hash("old"),
        dismissal.clone(),
    );
    assert_eq!(status(&stale), 409, "{stale}");
    assert_eq!(read(&dir, LEDGER), TEXT);

    let body = json!({ "feature": "nope", "id": "F-1", "hash": "x", "status": dismissal });
    let unknown = post_json(&server, "/api/findings/status", &cookies, &body.to_string());
    assert_eq!(status(&unknown), 404, "{unknown}");
    server.stop().unwrap();
}

#[test]
fn a_stale_hash_conflicts_before_the_new_text_is_built() {
    if skip("a_stale_hash_conflicts_before_the_new_text_is_built") {
        return;
    }
    let dir = demo(&[(LEDGER, TEXT)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    // An unknown id would be a 400 if the text were built before the hash is compared.
    let dismissal = json!({ "kind": "dismissed", "detail": "no" });
    let stale = post_status(&server, &cookies, "F-9", &normalized_hash("old"), dismissal);
    assert_eq!(status(&stale), 409, "{stale}");
    assert_eq!(read(&dir, LEDGER), TEXT);
    server.stop().unwrap();
}

#[test]
fn a_feature_without_a_ledger_is_not_found() {
    if skip("a_feature_without_a_ledger_is_not_found") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let response = post_status(
        &server,
        &cookies,
        "F-1",
        "x",
        json!({ "kind": "open", "detail": null }),
    );
    assert_eq!(status(&response), 404, "{response}");
    server.stop().unwrap();
}

#[test]
fn status_rules() {
    let anchors = vec!["1-first".to_owned()];
    let ok = |kind, detail| status_of(&view(kind, detail), &anchors);
    assert_eq!(ok(FindingKindView::Open, None), Ok(FindingStatus::Open));
    assert_eq!(
        ok(FindingKindView::Planned, Some(" PLAN-a_b.c-1.md ")),
        Ok(FindingStatus::Planned("PLAN-a_b.c-1.md".into()))
    );
    for bad in [
        "PLAN-a.md\nx",
        "DONE-PLAN-a.md",
        "PLAN-a/b.md",
        "PLAN-.hidden.md",
        "plan-a.md",
    ] {
        assert!(ok(FindingKindView::Planned, Some(bad)).is_err(), "{bad}");
    }
    assert_eq!(
        ok(FindingKindView::Folded, Some("metaplan#1-first")),
        Ok(FindingStatus::Folded("metaplan#1-first".into()))
    );
    assert!(ok(FindingKindView::Folded, Some("metaplan#2-second")).is_err());
    assert!(ok(FindingKindView::Folded, Some("1-first")).is_err());
    assert_eq!(
        ok(FindingKindView::Dismissed, Some("not needed\n## F-2 x")),
        Ok(FindingStatus::Dismissed("not needed ## F-2 x".into()))
    );
    assert!(ok(FindingKindView::Dismissed, Some("")).is_err());
    assert!(ok(FindingKindView::Dismissed, None).is_err());
}
