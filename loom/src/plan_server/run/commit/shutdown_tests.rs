//! A commit accepted over HTTP remains owned until its detached Git job finishes.

use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use super::Repo;
use crate::plan_server::lock::Acquired;
use crate::plan_server::tests::support as http;
use crate::plan_server::RunningServer;
use crate::web_server::ShutdownPending;

fn wait_for_hook(repo: &Repo) {
    let marker = repo.0.path().join("hook-entered");
    let deadline = Instant::now() + http::WAIT;
    while !marker.exists() {
        assert!(Instant::now() < deadline, "commit hook did not start");
        thread::sleep(Duration::from_millis(10));
    }
}

fn singleton_held(path: &std::path::Path) -> bool {
    matches!(
        crate::plan_server::lock::acquire(path).expect("check singleton lock"),
        Acquired::Running(_)
    )
}

/// A repository whose dirty tracked file commits through a pre-commit hook that
/// blocks until `hook-release` appears. Returns the repository and its HEAD.
fn repo_with_blocking_hook() -> (Repo, String) {
    let repo = Repo::new();
    repo.put("doc/product/index.md", "# Original\n");
    repo.git(&["add", "--", "doc/product/index.md"]);
    repo.git(&["commit", "-q", "-m", "product baseline"]);
    repo.put("doc/product/index.md", "# Updated\n");
    repo.hook(
        "pre-commit",
        "#!/bin/sh\nprintf entered > hook-entered\nn=0\nwhile [ ! -e hook-release ]; do\n  n=$((n + 1))\n  [ \"$n\" -lt 600 ] || exit 1\n  sleep 0.02\ndone\n",
    );
    let head = repo.git(&["rev-parse", "HEAD"]);
    (repo, head)
}

/// Hold the singleton through `server`, as the real server process does.
fn retain_singleton(server: &RunningServer, lock_path: &std::path::Path) {
    let Acquired::Held(lock) =
        crate::plan_server::lock::acquire(lock_path).expect("acquire singleton lock")
    else {
        panic!("new singleton lock must be free");
    };
    server.ownership.retain(lock);
}

/// Take the workspace over HTTP and start a commit that parks inside its hook.
fn start_blocked_commit(server: &RunningServer, repo: &Repo) {
    let cookies = http::bootstrap(server);
    let origin = http::origin(server);
    let take = http::request("POST", server, "/api/test/take", &cookies, &[&origin], "");
    assert_eq!(
        http::status(&http::send(server.local, take.as_bytes())),
        200
    );
    let response = http::post_json(
        server,
        "/api/run/commit",
        &cookies,
        r#"{"paths":["doc/product/index.md"],"message":"Save docs"}"#,
    );
    assert_eq!(http::status(&response), 202, "{response}");
    wait_for_hook(repo);
}

#[test]
#[serial_test::serial]
fn accepted_http_commit_keeps_port_and_singleton_through_bounded_stop() {
    if http::skip("accepted_http_commit_keeps_port_and_singleton_through_bounded_stop") {
        return;
    }
    let (repo, original_head) = repo_with_blocking_hook();
    let server = http::serve(&repo.0);
    let local = server.local;
    let lock_path = repo.0.path().join("server.lock");
    retain_singleton(&server, &lock_path);
    start_blocked_commit(&server, &repo);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), original_head);

    let pending = server
        .stop()
        .expect_err("blocked commit must outlive bounded stop");
    let pending = pending
        .downcast::<ShutdownPending>()
        .expect("shutdown must return its pending owner");
    assert!(!pending.is_complete());
    assert!(
        TcpListener::bind(local).is_err(),
        "port released during commit"
    );
    assert!(
        singleton_held(&lock_path),
        "singleton released during commit"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), original_head);

    repo.put("hook-release", "go\n");
    assert!(pending.wait(http::WAIT), "commit did not drain");
    assert_eq!(pending.completion_error(), None);
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "Save docs\n");
    assert_eq!(
        repo.git(&["show", "HEAD:doc/product/index.md"]),
        "# Updated\n"
    );
    assert!(!singleton_held(&lock_path), "singleton stayed held");
    TcpListener::bind(local).expect("port released after commit");
}
