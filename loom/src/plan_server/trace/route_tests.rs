//! Route tests of the settle, trace and settled-version routes, over a served DEMO project in
//! a git repository (the six route contracts live in `contract_tests.rs`, the settle's commit in
//! `commit_route_tests.rs`).

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::plan_server::tests::support::{
    bootstrap, get, json_body, post_json, serve, skip, status,
};
use crate::plan_server::RunningServer;
use crate::planning::sections::normalized_hash;

use super::settle_commit_tests::configure;

pub(super) const SECTION_01: &str = "doc/product/demo/section_01.md";
pub(super) const SECTION_REL: &str = "product/demo/section_01.md";

pub(super) fn put(root: &Path, rel: &str, content: &str) {
    std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    std::fs::write(root.join(rel), content).unwrap();
}

pub(super) fn read(dir: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(dir.path().join(rel)).unwrap()
}

/// Run git in `root`; the output is the trimmed stdout.
pub(super) fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A two-section feature with a metaplan and one plan, plus `extra` files written over it,
/// committed once in a new git repository whose own configuration names the committer.
pub(super) fn demo(extra: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let files = [
        (
            "doc/product/demo/index.md",
            "# Demo\n\nA demo.\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n\
             | 01 | Alpha |\n| 02 | Beta |\n",
        ),
        (
            SECTION_01,
            "# Alpha\n\n## One\n\nText one.\n\n## Two\n\nText two.\n",
        ),
        (
            "doc/product/demo/section_02.md",
            "# Beta\n\n## Three\n\nText.\n",
        ),
        (
            "doc/plans/demo/index.md",
            "# Engineering\n\n## 1. First (`PLAN-first.md`)\n\nCovers: section_01#one\n\n\
             First plan.\n\n## 2. Second (`PLAN-second.md`)\n\nCovers: section_02\n\nSecond.\n",
        ),
        (
            "doc/plans/demo/PLAN-first.md",
            "# First\n\nImplements: section_01#one\n\nBody.\n",
        ),
    ];
    for (rel, text) in files.iter().chain(extra) {
        put(root, rel, text);
    }
    git(root, &["init", "-q"]);
    configure(root);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "demo"]);
    dir
}

pub(super) fn settle(
    server: &RunningServer,
    cookies: &str,
    rel: &str,
    hash: &str,
    extra: &str,
) -> String {
    let target = format!("/api/settle?path={rel}&hash={hash}{extra}");
    post_json(server, &target, cookies, "")
}

#[test]
fn settle_with_a_stale_hash_conflicts_and_writes_nothing() {
    if skip("settle_with_a_stale_hash_conflicts_and_writes_nothing") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let before = read(&dir, SECTION_01);
    let response = settle(
        &server,
        &cookies,
        SECTION_REL,
        &normalized_hash("older"),
        "",
    );
    assert_eq!(status(&response), 409, "{response}");
    let body = json_body(&response);
    assert_eq!(body["result"], "conflict", "{response}");
    assert_eq!(body["current_hash"], normalized_hash(&before).as_str());
    assert_eq!(read(&dir, SECTION_01), before);
    server.stop().unwrap();
}

/// The watcher compares size, mtime and inode, so an in-place same-size rewrite that restores
/// the mtime leaves the loaded project on the older text for as long as the test needs.
#[test]
fn settle_answers_stale_while_the_loaded_project_trails_the_file() {
    if skip("settle_answers_stale_while_the_loaded_project_trails_the_file") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let path = dir.path().join(SECTION_01);
    let before = read(&dir, SECTION_01);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let edited = before.replace("Text one.", "Text uno.");
    assert_eq!(edited.len(), before.len());
    std::fs::write(&path, &edited).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let response = settle(
        &server,
        &cookies,
        SECTION_REL,
        &normalized_hash(&edited),
        "",
    );
    assert_eq!(status(&response), 409, "{response}");
    assert_eq!(json_body(&response)["error"], "stale", "{response}");
    assert_eq!(read(&dir, SECTION_01), edited);
    server.stop().unwrap();
}

#[test]
fn settle_is_refused_while_another_browser_holds_authoring() {
    if skip("settle_is_refused_while_another_browser_holds_authoring") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let holder = bootstrap(&server);
    let reader = bootstrap(&server);
    let taken = post_json(&server, "/api/test/take", &holder, "");
    assert_eq!(status(&taken), 200, "{taken}");
    let before = read(&dir, SECTION_01);
    let refused = settle(&server, &reader, SECTION_REL, &normalized_hash(&before), "");
    assert_eq!(status(&refused), 403, "{refused}");
    assert_eq!(read(&dir, SECTION_01), before);
    server.stop().unwrap();
}

#[test]
fn settle_refuses_a_stub_and_a_path_outside_the_trace() {
    if skip("settle_refuses_a_stub_and_a_path_outside_the_trace") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    let hash = normalized_hash("");
    for rel in [
        "plans/demo/PLAN-second.md",
        "loom/knowledge/x.md",
        "plans/demo/nope.md",
    ] {
        let response = settle(&server, &cookies, rel, &hash, "");
        assert_eq!(status(&response), 400, "{rel}: {response}");
    }
    let bad = settle(&server, &cookies, "product/../x.md", &hash, "");
    assert_eq!(status(&bad), 400, "{bad}");
    server.stop().unwrap();
}

#[test]
fn settled_version_rejects_a_bad_or_unknown_path() {
    if skip("settled_version_rejects_a_bad_or_unknown_path") {
        return;
    }
    let dir = demo(&[]);
    let server = serve(&dir);
    let cookies = bootstrap(&server);
    for (path, want) in [("a/../b.md", 400), ("product/demo/nope.md", 404)] {
        let response = get(
            &server,
            &format!("/api/settled-version?path={path}"),
            &cookies,
        );
        assert_eq!(status(&response), want, "{path}: {response}");
    }
    let missing = get(&server, "/api/settled-version", &cookies);
    assert_eq!(status(&missing), 400, "{missing}");
    server.stop().unwrap();
}
