//! The planning-server stop API preserves the shared transport's pending owner.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::support;
use crate::plan_server::request::{ApiRequest, ApiResponse};
use crate::plan_server::routes::{BodyKind, Method, Role, RouteSpec};
use crate::web_server::ShutdownPending;

struct Paused {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

fn update(request: &ApiRequest<'_>) -> ApiResponse {
    let paused = request.ctx.extensions().get::<Paused>().unwrap();
    paused.entered.send(()).unwrap();
    paused
        .release
        .lock()
        .unwrap()
        .recv_timeout(support::WAIT)
        .unwrap();
    std::fs::write(request.ctx.root().join("pending-update"), "committed").unwrap();
    ApiResponse::json(200, &serde_json::json!({ "ok": true }))
}

fn serve_paused(
    project: &tempfile::TempDir,
) -> (
    crate::plan_server::RunningServer,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let route = |path, body| RouteSpec {
        method: Method::Post,
        path,
        body,
        role: Role::Viewer,
        handler: update,
    };
    let routes = vec![
        route("/api/test/pending", BodyKind::None),
        route("/api/test/pending-body", BodyKind::Json),
    ];
    let server = crate::plan_server::start_with_routes(support::options(project), routes).unwrap();
    let (entered, admitted) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    server.ctx.extensions().insert(Arc::new(Paused {
        entered,
        release: Mutex::new(wait),
    }));
    (server, admitted, release)
}

fn begin_request(server: &crate::plan_server::RunningServer) -> mpsc::Receiver<String> {
    let local = server.local;
    let cookies = support::bootstrap(server);
    let origin = support::origin(server);
    let request = support::request(
        "POST",
        server,
        "/api/test/pending",
        &cookies,
        &[&origin],
        "",
    );
    let (response_tx, response) = mpsc::channel();
    thread::spawn(move || {
        response_tx
            .send(support::send(local, request.as_bytes()))
            .unwrap()
    });
    response
}

fn retain_singleton(
    server: &crate::plan_server::RunningServer,
    project: &tempfile::TempDir,
) -> std::path::PathBuf {
    let path = project.path().join("server.lock");
    let crate::plan_server::lock::Acquired::Held(lock) =
        crate::plan_server::lock::acquire(&path).unwrap()
    else {
        panic!("new singleton lock must be free")
    };
    server.ownership.retain(lock);
    path
}

fn assert_singleton(path: &std::path::Path, held: bool) {
    assert_eq!(
        matches!(
            crate::plan_server::lock::acquire(path).unwrap(),
            crate::plan_server::lock::Acquired::Running(_)
        ),
        held
    );
}

#[test]
fn stop_propagates_pending_and_retains_request_context() {
    if support::skip("stop_propagates_pending_and_retains_request_context") {
        return;
    }
    let project = support::project();
    let (server, admitted, release) = serve_paused(&project);
    let lock_path = retain_singleton(&server, &project);
    let local = server.local;
    let response = begin_request(&server);
    admitted.recv_timeout(support::WAIT).unwrap();
    let (stopped_tx, stopped) = mpsc::channel();
    thread::spawn(move || stopped_tx.send(server.stop()).unwrap());
    let error = stopped.recv_timeout(support::WAIT).unwrap().unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    assert!(!pending.is_complete());
    assert!(TcpListener::bind(local).is_err());
    assert_singleton(&lock_path, true);
    assert!(!project.path().join("pending-update").exists());
    release.send(()).unwrap();
    assert_eq!(
        support::status(&response.recv_timeout(support::WAIT).unwrap()),
        200
    );
    assert!(pending.wait(support::WAIT));
    assert_singleton(&lock_path, false);
    assert_eq!(
        std::fs::read_to_string(project.path().join("pending-update")).unwrap(),
        "committed"
    );
    TcpListener::bind(local).expect("listener released only after the route finishes");
}

#[test]
fn incomplete_body_is_cancelled_by_stop_and_never_reaches_its_handler() {
    if support::skip("incomplete_body_is_cancelled_by_stop_and_never_reaches_its_handler") {
        return;
    }
    let project = support::project();
    let (server, _admitted, _release) = serve_paused(&project);
    let cookies = support::bootstrap(&server);
    let origin = support::origin(&server);
    let raw = support::request(
        "POST",
        &server,
        "/api/test/pending-body",
        &cookies,
        &[&origin, "Content-Type: application/json"],
        r#"{"complete":false}"#,
    );
    let mut client = TcpStream::connect(server.local).unwrap();
    client.set_read_timeout(Some(support::WAIT)).unwrap();
    client.write_all(&raw.as_bytes()[..raw.len() - 4]).unwrap();
    // Give the connection worker time to pass the gate and enter the body read.
    thread::sleep(Duration::from_millis(250));

    server
        .stop()
        .expect("an incomplete body must not outlive stop");

    let _ = client.write_all(&raw.as_bytes()[raw.len() - 4..]);
    let mut response = Vec::new();
    let _ = client.read_to_end(&mut response);
    assert_eq!(support::status(&String::from_utf8_lossy(&response)), 503);
    assert!(!project.path().join("pending-update").exists());
}

#[test]
fn blocked_watcher_keeps_bounded_stop_pending_and_owned() {
    if support::skip("blocked_watcher_keeps_bounded_stop_pending_and_owned") {
        return;
    }
    let project = support::project();
    let server = support::serve(&project);
    let local = server.local;
    let lock_path = retain_singleton(&server, &project);
    let (entered_tx, entered) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    server.ctx.on_batch(Box::new(move |ctx, _| {
        entered_tx.send(()).unwrap();
        wait.lock().unwrap().recv_timeout(support::WAIT).unwrap();
        std::fs::write(ctx.root().join("watcher-update"), "committed").unwrap();
    }));
    std::fs::write(project.path().join("doc/product/index.md"), "# Changed\n").unwrap();
    entered.recv_timeout(support::WAIT).unwrap();
    let (stopped_tx, stopped) = mpsc::channel();
    thread::spawn(move || stopped_tx.send(server.stop()).unwrap());
    let error = stopped
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    assert!(!pending.is_complete());
    assert!(TcpListener::bind(local).is_err());
    assert_singleton(&lock_path, true);
    assert!(!project.path().join("watcher-update").exists());
    release.send(()).unwrap();
    assert!(pending.wait(support::WAIT));
    assert!(project.path().join("watcher-update").exists());
    assert_singleton(&lock_path, false);
    TcpListener::bind(local).expect("listener released only after the watcher finishes");
}

struct BlockingDrop {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl Drop for BlockingDrop {
    fn drop(&mut self) {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(support::WAIT)
            .unwrap();
    }
}

#[test]
fn context_disposal_runs_under_the_bounded_cleanup_owner() {
    if support::skip("context_disposal_runs_under_the_bounded_cleanup_owner") {
        return;
    }
    let project = support::project();
    let server = support::serve(&project);
    let local = server.local;
    let lock_path = retain_singleton(&server, &project);
    let (entered_tx, entered) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    server.ctx.extensions().insert(Arc::new(BlockingDrop {
        entered: entered_tx,
        release: Mutex::new(wait),
    }));
    let (stopped_tx, stopped) = mpsc::channel();
    thread::spawn(move || stopped_tx.send(server.stop()).unwrap());
    entered.recv_timeout(support::WAIT).unwrap();
    let error = stopped
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    assert!(!pending.is_complete());
    assert!(TcpListener::bind(local).is_err());
    assert_singleton(&lock_path, true);
    release.send(()).unwrap();
    assert!(pending.wait(support::WAIT));
    assert_eq!(pending.completion_error(), None);
    assert_singleton(&lock_path, false);
    TcpListener::bind(local).expect("listener released only after context disposal");
}
