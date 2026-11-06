//! Contracts for the trace routes: `GET /api/trace/<feature>`, `POST /api/settle`,
//! `POST /api/findings/status` and the `trace_changed` message, over a served DEMO project
//! inside a git repository.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde_json::{json, Value};
use tempfile::TempDir;
use tungstenite::Message;

use crate::plan_server::tests::support::{
    bootstrap, connect_ws, get, json_body, origin, post_json, read_until, serve, skip, status, WAIT,
};
use crate::plan_server::RunningServer;
use crate::planning::sections::normalized_hash;

const SECTION_01: &str = "doc/product/demo/section_01.md";
const SECTION_02: &str = "doc/product/demo/section_02.md";
const LEDGER: &str = "doc/plans/demo/findings.md";

fn put(root: &Path, rel: &str, content: &str) {
    std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    std::fs::write(root.join(rel), content).unwrap();
}

fn read(dir: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(dir.path().join(rel)).unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// The DEMO tree of `loom/tests/planning_trace_contracts.rs` with `extra` files written over
/// it (an empty text removes a file), committed once in a new git repository.
fn demo(extra: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "doc/product/demo/index.md",
        "# Demo\n\nA demo.\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n\
         | 01 | Alpha |\n| 02 | Beta |\n",
    );
    put(
        root,
        SECTION_01,
        "# Alpha\n\n## One\n\nText one.\n\n## Two\n\nText two.\n\n## Open questions\n\nNone.\n",
    );
    put(root, SECTION_02, "# Beta\n\n## Three\n\nText three.\n");
    put(
        root,
        "doc/plans/demo/index.md",
        "# Engineering\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n\n\
         First plan.\n\n## 2. Second (`PLAN-second.md`)\n\nCovers: section_02\n\nSecond plan.\n",
    );
    put(
        root,
        "doc/plans/demo/PLAN-first.md",
        "# First\n\nImplements: section_01#one\n\nBody.\n",
    );
    for (rel, text) in extra {
        if text.is_empty() {
            std::fs::remove_file(root.join(rel)).unwrap();
        } else {
            put(root, rel, text);
        }
    }
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "demo"]);
    dir
}

/// `POST /api/settle` for `doc/<rel>` at the hash of `opened`, with `extra` query pairs.
fn settle(server: &RunningServer, cookies: &str, rel: &str, opened: &str, extra: &str) -> String {
    let hash = normalized_hash(opened);
    let target = format!("/api/settle?path={rel}&hash={hash}{extra}");
    post_json(server, &target, cookies, "")
}

fn trace(server: &RunningServer, cookies: &str) -> Value {
    let response = get(server, "/api/trace/demo", cookies);
    assert_eq!(status(&response), 200, "{response}");
    json_body(&response)
}

/// Wait (bounded by [`WAIT`]) until the served trace shows `path` in settle state `want`.
fn wait_for_settle(server: &RunningServer, cookies: &str, path: &str, want: &str) {
    let start = Instant::now();
    loop {
        let view = trace(server, cookies);
        let docs = view["documents"].as_array().expect("documents array");
        let state = docs.iter().find(|doc| doc["path"] == path);
        if state.is_some_and(|doc| doc["settle"] == want) {
            return;
        }
        assert!(start.elapsed() < WAIT, "{path} never read {want}: {view}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// `name` at the top level of `value` or one object below it.
fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    value.get(name).or_else(|| {
        value
            .as_object()?
            .values()
            .find_map(|child| child.as_object()?.get(name))
    })
}

#[test]
fn trace_route_serves_the_feature() {
    if skip("trace_route_serves_the_feature") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let view = trace(&server, &cookies);
    let headings = view["headings"].as_array().expect("headings array");
    assert!(
        headings.iter().any(|h| h["path"] == SECTION_01
            && h["anchor"] == "two"
            && h["covered_by"] == json!([])),
        "headings: {view}"
    );
    let node = json!({ "path": SECTION_01, "anchor": "two" });
    let gaps = view["gaps"].as_array().expect("gaps array");
    assert!(
        gaps.iter()
            .any(|gap| gap["kind"] == "uncovered" && gap["node"] == node),
        "gaps: {view}"
    );
    let missing = get(&server, "/api/trace/nope", &cookies);
    assert_eq!(status(&missing), 404, "{missing}");
    server.stop().unwrap();
}

#[test]
fn settle_writes_the_settled_line_once() {
    if skip("settle_writes_the_settled_line_once") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let opened = read(&dir, SECTION_01);
    let first = settle(&server, &cookies, "product/demo/section_01.md", &opened, "");
    assert_eq!(status(&first), 200, "{first}");
    let settled = read(&dir, SECTION_01);
    let marks = settled
        .lines()
        .filter(|l| l.starts_with("Settled: "))
        .count();
    assert_eq!(marks, 1, "file:\n{settled}");
    wait_for_settle(&server, &cookies, SECTION_01, "settled");

    let again = settle(&server, &cookies, "product/demo/section_01.md", &opened, "");
    assert_eq!(status(&again), 409, "{again}");
    assert_eq!(read(&dir, SECTION_01), settled);
    server.stop().unwrap();
}

#[test]
fn settle_needs_confirmation_for_my_calls() {
    if skip("settle_needs_confirmation_for_my_calls") {
        return;
    }
    let text = "# Beta\n\n## Three\n\nText three.\n\nUse a lock. (My call.)\n";
    let dir = demo(&[(SECTION_02, text)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let refused = settle(&server, &cookies, "product/demo/section_02.md", text, "");
    assert_eq!(status(&refused), 409, "{refused}");
    let value = json_body(&refused);
    assert_eq!(value["error"], "unresolved", "{refused}");
    assert_eq!(field(&value, "my_calls"), Some(&json!(1)), "{refused}");
    assert_eq!(read(&dir, SECTION_02), text);

    let path = "product/demo/section_02.md";
    let confirmed = settle(&server, &cookies, path, text, "&confirm=1");
    assert_eq!(status(&confirmed), 200, "{confirmed}");
    let written = read(&dir, SECTION_02);
    assert!(
        written.lines().any(|l| l.starts_with("Settled: ")),
        "file:\n{written}"
    );
    server.stop().unwrap();
}

#[test]
fn locked_plan_cannot_be_settled() {
    if skip("locked_plan_cannot_be_settled") {
        return;
    }
    let running = "doc/plans/demo/IN_PROGRESS-PLAN-first.md";
    let text = "# First\n\nImplements: section_01#one\n\nBody.\n";
    let dir = demo(&[("doc/plans/demo/PLAN-first.md", ""), (running, text)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let path = "plans/demo/IN_PROGRESS-PLAN-first.md";
    let refused = settle(&server, &cookies, path, text, "");
    assert_eq!(status(&refused), 403, "{refused}");
    assert_eq!(json_body(&refused)["error"], "locked", "{refused}");
    assert_eq!(read(&dir, running), text);
    server.stop().unwrap();
}

/// `POST /api/findings/status` dismissing `F-1` of the demo ledger with `detail`.
fn dismiss(server: &RunningServer, cookies: &str, ledger: &str, detail: &str) -> String {
    let body = json!({
        "feature": "demo",
        "id": "F-1",
        "hash": normalized_hash(ledger),
        "status": { "kind": "dismissed", "detail": detail },
    });
    post_json(server, "/api/findings/status", cookies, &body.to_string())
}

#[test]
fn finding_dismissal_is_one_line() {
    if skip("finding_dismissal_is_one_line") {
        return;
    }
    let ledger =
        "# Findings\n\n## F-1 Flag\n\nAbout: section_01#one\nStatus: open\n\nRename the flag.\n";
    let dir = demo(&[(LEDGER, ledger)]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let saved = dismiss(&server, &cookies, ledger, "not needed\n## F-2 x");
    assert_eq!(status(&saved), 200, "{saved}");
    let written = read(&dir, LEDGER);
    let lines: Vec<&str> = written.lines().collect();
    assert!(
        lines.contains(&"Status: dismissed: not needed ## F-2 x"),
        "ledger:\n{written}"
    );
    let entries = lines.iter().filter(|l| l.starts_with("## F-")).count();
    assert_eq!(entries, 1, "ledger:\n{written}");

    let empty = dismiss(&server, &cookies, &written, "");
    assert_eq!(status(&empty), 400, "{empty}");
    assert_eq!(read(&dir, LEDGER), written);
    server.stop().unwrap();
}

#[test]
fn trace_changed_follows_a_heading_edit() {
    if skip("trace_changed_follows_a_heading_edit") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let origin = origin(&server).replacen("Origin: ", "", 1);
    let mut socket = connect_ws(&server, &cookies, Some(&origin)).expect("an admitted socket");
    read_until(&mut socket, "hello");

    let text = format!("{}\n## Four\n\nText four.\n", read(&dir, SECTION_01));
    put(dir.path(), SECTION_01, &text);
    // Other frames (summaries, presence) keep the socket busy, so the read timeout alone
    // never ends the wait: a deadline does.
    let start = Instant::now();
    loop {
        assert!(start.elapsed() < WAIT, "no trace_changed frame for demo");
        let Message::Text(text) = socket.read().expect("read a frame") else {
            continue;
        };
        let frame: Value = serde_json::from_str(text.as_str()).expect("JSON frame");
        if frame["type"] == "trace_changed" && frame["feature"] == "demo" {
            break;
        }
    }
    server.stop().unwrap();
}
