//! The commit route: synchronous refusals, then the streamed job.

use std::time::Duration;

use super::super::*;
use super::{strings, Repo};
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::request::QueryParams;
use crate::web_server::http::RequestHead;

fn request(ctx: &PlanContext, paths: &[&str], message: &str) -> ApiResponse {
    let browser = BrowserId::mint().unwrap();
    let public_id = PublicId::mint();
    let head = RequestHead {
        method: "POST".into(),
        path: "/api/run/commit".into(),
        query: None,
        upgrade_websocket: false,
        origin: None,
        host: None,
        cookie: None,
        content_type: None,
        content_length: None,
        csrf_token: None,
    };
    post_commit(&ApiRequest {
        ctx,
        head: &head,
        browser: &browser,
        public_id: &public_id,
        query: QueryParams::parse(None).unwrap(),
        body: serde_json::to_vec(&CommitRequest {
            paths: strings(paths),
            message: message.into(),
        })
        .unwrap(),
    })
}

fn assert_refusal(response: ApiResponse, status: u16, code: &str) {
    assert_eq!(response.status, status);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["error"], code);
}

#[test]
#[serial_test::serial]
fn route_recomputes_current_paths_and_refuses_each_invalid_selection() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let ctx = repo.context(false);
    repo.put("doc/a.md", "original\n");
    for paths in [
        vec![],
        vec!["src.txt"],
        vec!["doc/a.md"],
        vec!["doc/missing.md"],
        vec!["doc/../src.txt"],
        vec!["/doc/a.md"],
    ] {
        assert_refusal(request(&ctx, &paths, "edit"), 400, "invalid_paths");
    }
    assert_eq!(child::jobs(&ctx).current(), None);
}

#[test]
#[serial_test::serial]
fn route_refuses_empty_and_oversized_messages() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let ctx = repo.context(false);
    for message in [String::new(), "x".repeat(2_001), "é".repeat(1_001)] {
        assert_refusal(
            request(&ctx, &["doc/a.md"], &message),
            400,
            "invalid_message",
        );
    }
    assert_eq!(child::jobs(&ctx).current(), None);
}

#[test]
#[serial_test::serial]
fn route_refuses_read_only() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let ctx = repo.context(true);
    assert_refusal(request(&ctx, &["doc/a.md"], "edit"), 403, "read_only");
    assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "");
}

#[test]
#[serial_test::serial]
fn route_refuses_a_concurrent_job() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let ctx = repo.context(false);
    let _guard = child::jobs(&ctx).begin(RunJob::Start).unwrap();
    assert_refusal(request(&ctx, &["doc/a.md"], "edit"), 409, "busy");
    assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "");
}

#[test]
#[serial_test::serial]
fn route_accepts_and_emits_output_then_failure_done() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    repo.hook("pre-commit", "#!/bin/sh\necho out\necho err >&2\nexit 1\n");
    let ctx = repo.context(false);
    let frames = ctx
        .hub()
        .register(&ctx.conn_ctx(BrowserId::mint().unwrap()));
    let head = repo.git(&["rev-parse", "HEAD"]);

    assert_eq!(request(&ctx, &["doc/a.md"], "edit").status, 202);

    let mut lines = Vec::new();
    let done = loop {
        let frame = frames.recv_timeout(Duration::from_secs(10)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["job"], "commit");
        assert_eq!(value["step"], "git commit");
        if value["done"] == true {
            break value;
        }
        lines.push(value["line"].as_str().unwrap().to_owned());
    };
    assert_eq!(done["exit_code"], 1);
    // Both hook streams arrive whatever order the two pipes were drained in.
    lines.sort();
    assert_eq!(lines, ["err", "out"]);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
}

#[test]
#[serial_test::serial]
fn route_refuses_a_nul_in_the_message() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let ctx = repo.context(false);
    assert_refusal(
        request(&ctx, &["doc/a.md"], "edit\0more"),
        400,
        "invalid_message",
    );
    assert_eq!(child::jobs(&ctx).current(), None);
    assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "");
}

#[test]
#[serial_test::serial]
fn worker_recomputes_the_allowlist_and_reports_a_vanished_path() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let ctx = repo.context(false);
    let frames = ctx
        .hub()
        .register(&ctx.conn_ctx(BrowserId::mint().unwrap()));
    let head = repo.git(&["rev-parse", "HEAD"]);
    repo.put("doc/a.md", "original\n");

    commit_worker(
        &ctx,
        CommitRequest {
            paths: strings(&["doc/a.md"]),
            message: "edit".into(),
        },
    );

    let mut values = Vec::new();
    while let Ok(frame) = frames.recv_timeout(Duration::from_secs(1)) {
        values.push(serde_json::from_str::<serde_json::Value>(&frame).unwrap());
    }
    let (done, lines) = values.split_last().unwrap();
    assert_eq!(done["done"], true);
    assert_eq!(done["exit_code"], -1);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["stream"], "stderr");
    assert_eq!(lines[0]["line"], "invalid documentation paths");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
}
