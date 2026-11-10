//! The settle's commit over a served DEMO project: `POST /api/settle` answers once the line is
//! written, the commit runs after it, and how it ended reaches the browsers as a `settle_commit`
//! frame; `GET /api/settled-version` then finds the commit.

use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::TempDir;
use tungstenite::Message;

use crate::plan_server::tests::support::{
    bootstrap, connect_ws, get, json_body, origin, read_until, serve, skip, status, Socket, WAIT,
};
use crate::plan_server::RunningServer;
use crate::planning::sections::normalized_hash;

use super::route_tests::{demo, git, put, read, settle, SECTION_01, SECTION_REL};
use super::settle_commit_tests::{pre_commit, refusing_hook};

/// A new browser of `server`: its cookies, and its socket past `hello`.
fn browser(server: &RunningServer) -> (String, Socket) {
    let cookies = bootstrap(server);
    let origin = origin(server).replacen("Origin: ", "", 1);
    let mut socket = connect_ws(server, &cookies, Some(&origin)).expect("an admitted socket");
    read_until(&mut socket, "hello");
    (cookies, socket)
}

/// The `settle_commit` frame of `job`. Other frames keep the socket busy, so a deadline, not the
/// read timeout, ends the wait.
fn outcome(socket: &mut Socket, job: &str) -> Value {
    let start = Instant::now();
    loop {
        assert!(
            start.elapsed() < WAIT,
            "no settle_commit frame for job {job}"
        );
        let Message::Text(text) = socket.read().expect("read a frame") else {
            continue;
        };
        let frame: Value = serde_json::from_str(text.as_str()).expect("JSON frame");
        if frame["type"] == "settle_commit" && frame["job"] == job {
            return frame;
        }
    }
}

/// The answer's queued job.
fn pending_job(answer: &Value) -> String {
    assert_eq!(answer["result"], "saved", "{answer}");
    assert_eq!(answer["commit"]["state"], "pending", "{answer}");
    answer["commit"]["job"].as_str().expect("a job").to_owned()
}

/// Settle `rel` at `hash` (`extra`: more query pairs) and wait for its commit to end.
fn settle_and_wait(
    server: &RunningServer,
    cookies: &str,
    socket: &mut Socket,
    rel: &str,
    hash: &str,
    extra: &str,
) -> Value {
    let response = settle(server, cookies, rel, hash, extra);
    assert_eq!(status(&response), 200, "{response}");
    let job = pending_job(&json_body(&response));
    outcome(socket, &job)
}

fn settled_version(server: &RunningServer, cookies: &str, rel: &str) -> Value {
    let response = get(server, &format!("/api/settled-version?path={rel}"), cookies);
    assert_eq!(status(&response), 200, "{response}");
    json_body(&response)["commit"].clone()
}

fn head(dir: &TempDir) -> String {
    git(dir.path(), &["rev-parse", "HEAD"])
}

/// The subjects of the repository's commits, newest first.
fn subjects(dir: &TempDir) -> Vec<String> {
    git(dir.path(), &["log", "--format=%s"])
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_answer_comes_before_a_slow_hook_ends_and_the_frame_after() {
    if skip("the_answer_comes_before_a_slow_hook_ends_and_the_frame_after") {
        return;
    }
    let dir = demo(&[]);
    pre_commit(dir.path(), "sleep 3\n");
    let server = serve(&dir);
    let (cookies, mut socket) = browser(&server);
    let before = head(&dir);
    let hash = normalized_hash(&read(&dir, SECTION_01));
    let asked = Instant::now();
    let response = settle(&server, &cookies, SECTION_REL, &hash, "");
    assert!(
        asked.elapsed() < Duration::from_secs(3),
        "the answer waited on the hook"
    );
    assert_eq!(status(&response), 200, "{response}");
    let answer = json_body(&response);
    let job = pending_job(&answer);
    assert_eq!(head(&dir), before, "the hook is still running");
    let frame = outcome(&mut socket, &job);
    assert_eq!(frame["state"], "committed", "{frame}");
    assert_eq!(frame["path"], SECTION_01);
    assert_eq!(frame["commit"], head(&dir).as_str());
    assert_eq!(frame["reason"], Value::Null);
    server.stop().unwrap();
}

#[test]
fn settle_and_unsettle_each_commit_the_document() {
    if skip("settle_and_unsettle_each_commit_the_document") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let (cookies, mut socket) = browser(&server);
    let original = read(&dir, SECTION_01);
    let hash = normalized_hash(&original);
    let first = settle_and_wait(&server, &cookies, &mut socket, SECTION_REL, &hash, "");
    assert_eq!(first["state"], "committed", "{first}");
    let settled = read(&dir, SECTION_01);
    assert!(settled.contains("\nSettled: "), "{settled}");
    let hash = normalized_hash(&settled);
    let cleared = settle_and_wait(
        &server,
        &cookies,
        &mut socket,
        SECTION_REL,
        &hash,
        "&clear=1",
    );
    assert_eq!(read(&dir, SECTION_01), original);
    assert_eq!(cleared["state"], "committed", "{cleared}");
    assert_eq!(cleared["commit"], head(&dir).as_str());
    assert_eq!(
        subjects(&dir),
        [
            "docs(demo): unsettle product/demo/section_01.md",
            "docs(demo): settle product/demo/section_01.md",
            "demo",
        ]
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    server.stop().unwrap();
}

#[test]
fn a_plan_settle_commits_its_new_brief() {
    if skip("a_plan_settle_commits_its_new_brief") {
        return;
    }
    let dir = demo(&[]);
    let brief = "doc/plans/demo/briefs/first/common.md";
    put(dir.path(), brief, "# Brief\n\nShared by every worker.\n");
    let server = serve(&dir);
    let (cookies, mut socket) = browser(&server);
    let plan = "doc/plans/demo/PLAN-first.md";
    let hash = normalized_hash(&read(&dir, plan));
    let rel = "plans/demo/PLAN-first.md";
    let frame = settle_and_wait(&server, &cookies, &mut socket, rel, &hash, "");
    assert_eq!(frame["state"], "committed", "{frame}");
    let files = git(dir.path(), &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files.lines().collect::<Vec<_>>(), [plan, brief]);
    assert_eq!(
        subjects(&dir)[0],
        "docs(demo): settle plans/demo/PLAN-first.md"
    );
    server.stop().unwrap();
}

#[test]
fn a_refused_commit_keeps_the_written_line_and_says_why() {
    if skip("a_refused_commit_keeps_the_written_line_and_says_why") {
        return;
    }
    let dir = demo(&[]);
    refusing_hook(dir.path(), "settle later");
    let server = serve(&dir);
    let (cookies, mut socket) = browser(&server);
    let before = head(&dir);
    let hash = normalized_hash(&read(&dir, SECTION_01));
    let frame = settle_and_wait(&server, &cookies, &mut socket, SECTION_REL, &hash, "");
    assert_eq!(frame["state"], "not_committed", "{frame}");
    assert_eq!(frame["commit"], Value::Null, "{frame}");
    let reason = frame["reason"].as_str().expect("a reason");
    assert!(reason.contains("settle later"), "{reason}");
    assert!(read(&dir, SECTION_01).contains("\nSettled: "));
    assert_eq!(head(&dir), before);
    // Never committed: there is no settled version to compare with.
    assert_eq!(settled_version(&server, &cookies, SECTION_REL), Value::Null);
    server.stop().unwrap();
}

#[test]
fn settled_version_finds_the_settle_commit_once_it_ends() {
    if skip("settled_version_finds_the_settle_commit_once_it_ends") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let (cookies, mut socket) = browser(&server);
    assert_eq!(settled_version(&server, &cookies, SECTION_REL), Value::Null);
    let hash = normalized_hash(&read(&dir, SECTION_01));
    let frame = settle_and_wait(&server, &cookies, &mut socket, SECTION_REL, &hash, "");
    assert_eq!(frame["commit"], head(&dir).as_str(), "{frame}");
    assert_eq!(
        settled_version(&server, &cookies, SECTION_REL),
        frame["commit"]
    );
    server.stop().unwrap();
}

#[test]
fn settled_version_skips_a_newer_commit_that_removed_the_line() {
    if skip("settled_version_skips_a_newer_commit_that_removed_the_line") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let (cookies, mut socket) = browser(&server);
    let original = read(&dir, SECTION_01);
    let hash = normalized_hash(&original);
    let frame = settle_and_wait(&server, &cookies, &mut socket, SECTION_REL, &hash, "");
    let holding = head(&dir);
    assert_eq!(frame["commit"], holding.as_str(), "{frame}");
    let settled = read(&dir, SECTION_01);
    // A later commit clears the line; the same line is then written again, uncommitted.
    put(dir.path(), SECTION_01, &original);
    git(dir.path(), &["add", SECTION_01]);
    git(dir.path(), &["commit", "-q", "-m", "clear"]);
    put(dir.path(), SECTION_01, &settled);
    assert_eq!(
        settled_version(&server, &cookies, SECTION_REL),
        holding.as_str()
    );
    server.stop().unwrap();
}

#[test]
fn job_ids_lead_with_a_token_of_their_server() {
    if skip("job_ids_lead_with_a_token_of_their_server") {
        return;
    }
    let first_job = |dir: &TempDir| {
        let server = serve(dir);
        let (cookies, mut socket) = browser(&server);
        let hash = normalized_hash(&read(dir, SECTION_01));
        let response = settle(&server, &cookies, SECTION_REL, &hash, "");
        let job = pending_job(&json_body(&response));
        outcome(&mut socket, &job);
        server.stop().unwrap();
        job
    };
    let (one, two) = (first_job(&demo(&[])), first_job(&demo(&[])));
    for job in [&one, &two] {
        let (token, number) = job.split_once('-').expect("token-number");
        assert_eq!(token.len(), 8, "{job}");
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()), "{job}");
        assert_eq!(number, "1", "{job}");
    }
    assert_ne!(one, two, "each server's ids are its own");
}

#[test]
fn a_hook_s_words_reach_the_settling_browser_alone() {
    if skip("a_hook_s_words_reach_the_settling_browser_alone") {
        return;
    }
    let dir = demo(&[]);
    refusing_hook(dir.path(), "settle later");
    let server = serve(&dir);
    let (cookies, mut settling) = browser(&server);
    let (_, mut watching) = browser(&server);
    let hash = normalized_hash(&read(&dir, SECTION_01));
    let response = settle(&server, &cookies, SECTION_REL, &hash, "");
    let job = pending_job(&json_body(&response));
    let own = outcome(&mut settling, &job);
    let reason = own["reason"].as_str().expect("a reason");
    assert!(reason.contains("settle later"), "{reason}");
    let shared = outcome(&mut watching, &job);
    assert_eq!(shared["state"], "not_committed", "{shared}");
    assert_eq!(shared["reason"], "the commit failed", "{shared}");
    server.stop().unwrap();
}

#[test]
fn shutdown_ends_the_queued_settles_and_waits_for_the_one_in_flight() {
    if skip("shutdown_ends_the_queued_settles_and_waits_for_the_one_in_flight") {
        return;
    }
    let dir = demo(&[]);
    pre_commit(dir.path(), "sleep 3\n");
    let server = serve(&dir);
    let (cookies, mut socket) = browser(&server);
    let queue = |rel: &str, doc: &str| {
        let hash = normalized_hash(&read(&dir, doc));
        pending_job(&json_body(&settle(&server, &cookies, rel, &hash, "")))
    };
    let running = queue(SECTION_REL, SECTION_01);
    let queued = queue(
        "product/demo/section_02.md",
        "doc/product/demo/section_02.md",
    );
    let drains = crate::plan_server::begin_shutdown(&server.ctx);
    let dropped = outcome(&mut socket, &queued);
    assert_eq!(dropped["state"], "not_committed", "{dropped}");
    assert_eq!(
        dropped["reason"],
        "loom plan stopped before the commit ran; settle again"
    );
    let finished = outcome(&mut socket, &running);
    assert_eq!(finished["state"], "committed", "{finished}");
    assert_eq!(finished["commit"], head(&dir).as_str());
    assert_eq!(
        subjects(&dir).len(),
        2,
        "the queued settle was not committed"
    );
    server.stop_when_drained(drains).unwrap();
}
