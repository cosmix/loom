//! Startup does not wait for the background load: the URL can be announced and the shell served
//! while the install step runs, and API requests and WebSockets wait for it instead of seeing a
//! half-installed server.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::Message;

use super::support::{
    bootstrap, connect_ws_at, get, host, json_body, options, project, request, send, skip, status,
    WAIT,
};
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::loading::Install;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::{routes, start_owned, Launch, RunningServer};
use crate::web_server::limits::MAX_HELD;

/// How long a held request is watched for an answer it must not give.
const HELD: Duration = Duration::from_millis(300);

/// A server whose install step waits for [`Blocked::release`]; the channel keeps the test from
/// hanging when `start_owned` waits for the install, so that ordering fails the first `expect`.
struct Blocked {
    server: RunningServer,
    release: Sender<()>,
}

fn start_blocked(project: &tempfile::TempDir, after: Install) -> Blocked {
    let options = options(project);
    let (release, released) = mpsc::channel::<()>();
    let (entered_tx, entered) = mpsc::channel();
    let install: Install = Box::new(move |ctx| {
        entered_tx.send(()).expect("the test waits for the install");
        let _ = released.recv();
        after(ctx);
    });
    let (started_tx, started) = mpsc::channel();
    thread::spawn(move || {
        let server = start_owned(Launch {
            root: options.root,
            host: options.host,
            port: options.port,
            read_only: Box::new(|| ReadOnly::from_problems(Vec::new())),
            install,
            routes: routes::all_routes(),
            ownership: Arc::default(),
        });
        let _ = started_tx.send(server);
    });
    let server = started
        .recv_timeout(WAIT)
        .expect("start_owned returns while the install is blocked")
        .expect("start the planning server");
    entered.recv_timeout(WAIT).expect("the install step runs");
    Blocked { server, release }
}

/// Send `path` on a thread of its own, so a held request does not hold the test.
fn get_later(server: &RunningServer, path: &str, cookies: &str) -> Receiver<String> {
    let addr: SocketAddr = server.local;
    let raw = request("GET", server, path, cookies, &[], "");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(send(addr, raw.as_bytes()));
    });
    rx
}

#[test]
fn the_shell_answers_and_data_routes_wait_while_the_install_runs() {
    if skip("the_shell_answers_and_data_routes_wait_while_the_install_runs") {
        return;
    }
    let project = project();
    let Blocked { server, release } = start_blocked(&project, Box::new(routes::install_all));

    let cookies = bootstrap(&server);
    assert_eq!(status(&get(&server, "/", &cookies)), 200);
    assert_eq!(status(&get(&server, "/api/tree", "")), 401);
    let search = get_later(&server, "/api/search?q=demo", &cookies);
    let tree = get_later(&server, "/api/tree", &cookies);
    assert!(
        search.recv_timeout(HELD).is_err(),
        "search must not answer before its index exists"
    );
    assert!(tree.try_recv().is_err(), "the tree must wait for the load");

    release.send(()).expect("the install waits");

    let search = search.recv_timeout(WAIT).expect("the held search answers");
    assert_eq!(status(&search), 200, "{search}");
    let hits = &json_body(&search)["hits"];
    assert_eq!(hits[0]["path"], "doc/product/index.md", "{search}");
    let tree = tree.recv_timeout(WAIT).expect("the held tree answers");
    assert_eq!(status(&tree), 200, "{tree}");
    server.stop().expect("stop");
}

#[test]
fn a_websocket_opened_during_the_load_gets_hello_first() {
    if skip("a_websocket_opened_during_the_load_gets_hello_first") {
        return;
    }
    let project = project();
    let Blocked { server, release } = start_blocked(&project, Box::new(routes::install_all));
    let cookies = bootstrap(&server);
    let origin = format!("http://{}", host(&server));
    let addr = server.local;
    let opening = thread::spawn(move || connect_ws_at(addr, &cookies, Some(origin.as_str())));
    thread::sleep(HELD);
    assert!(!opening.is_finished(), "the upgrade must wait for the load");

    release.send(()).expect("the install waits");

    let mut socket = opening.join().expect("connect").expect("the upgrade");
    let Message::Text(first) = socket.read().expect("a first frame") else {
        panic!("the first frame must be text");
    };
    let first: serde_json::Value = serde_json::from_str(first.as_str()).expect("JSON");
    assert_eq!(first["type"], "hello", "{first}");
    drop(socket);
    server.stop().expect("stop");
}

#[test]
fn stopping_during_the_load_answers_held_requests_and_still_stops() {
    if skip("stopping_during_the_load_answers_held_requests_and_still_stops") {
        return;
    }
    let project = project();
    let Blocked { server, release } = start_blocked(&project, Box::new(routes::install_all));
    let cookies = bootstrap(&server);
    let held = get_later(&server, "/api/tree", &cookies);
    assert!(held.recv_timeout(HELD).is_err(), "the tree waits");

    server.ctx.shutdown().mark_stopping();

    let answer = held
        .recv_timeout(WAIT)
        .expect("a stop releases held requests");
    assert_eq!(status(&answer), 503, "{answer}");
    assert_eq!(json_body(&answer)["error"], "stopping", "{answer}");
    release.send(()).expect("the install waits");
    server.stop().expect("stop");
}

#[test]
fn a_full_held_lane_answers_loading_at_once_and_keeps_the_shell_served() {
    if skip("a_full_held_lane_answers_loading_at_once_and_keeps_the_shell_served") {
        return;
    }
    let project = project();
    let Blocked { server, release } = start_blocked(&project, Box::new(routes::install_all));
    let cookies = bootstrap(&server);
    let held: Vec<_> = (0..MAX_HELD)
        .map(|_| get_later(&server, "/api/tree", &cookies))
        .collect();
    // Every request holds a connection thread until the load ends; let all of them reach it.
    thread::sleep(Duration::from_secs(1));
    assert!(held.iter().all(|answer| answer.try_recv().is_err()));

    let started = Instant::now();
    let refused = get(&server, "/api/tree", &cookies);
    // `get` itself stops reading at WAIT, so only a bound below it can catch a slow refusal.
    assert!(started.elapsed() < WAIT / 2, "the refusal must not wait");
    assert_eq!(status(&refused), 503, "{refused}");
    assert_eq!(json_body(&refused)["error"], "loading", "{refused}");
    assert_eq!(status(&get(&server, "/", &cookies)), 200);

    release.send(()).expect("the install waits");
    for answer in held {
        let answer = answer.recv_timeout(WAIT).expect("a held request answers");
        assert_eq!(status(&answer), 200, "{answer}");
    }
    server.stop().expect("stop");
}

#[test]
fn a_failed_load_refuses_held_requests_and_the_stop_reports_it() {
    if skip("a_failed_load_refuses_held_requests_and_the_stop_reports_it") {
        return;
    }
    let project = project();
    let panics: Install = Box::new(|_| panic!("injected install failure"));
    let Blocked { server, release } = start_blocked(&project, panics);
    let cookies = bootstrap(&server);
    let held = get_later(&server, "/api/tree", &cookies);
    assert!(held.recv_timeout(HELD).is_err(), "the tree waits");

    release.send(()).expect("the install waits");

    let answer = held.recv_timeout(WAIT).expect("the failure releases it");
    assert_eq!(status(&answer), 500, "{answer}");
    assert_eq!(json_body(&answer)["error"], "load_failed", "{answer}");
    // Exact text: the recorded failure can name host paths, and viewers may be remote.
    assert_eq!(
        json_body(&answer)["message"],
        "the project could not be loaded; see the loom plan terminal",
        "{answer}"
    );
    let error = server.stop().expect_err("the stop reports the failed load");
    assert!(
        format!("{error:#}").contains("could not be loaded"),
        "{error:#}"
    );
}

#[test]
fn an_edit_during_the_load_reaches_the_project_and_the_texts() {
    if skip("an_edit_during_the_load_reaches_the_project_and_the_texts") {
        return;
    }
    let project = project();
    let Blocked { server, release } = start_blocked(&project, Box::new(routes::install_all));
    let rel = Path::new("doc/product/index.md");
    let edited = "# Edited while loading\n";
    std::fs::write(project.path().join(rel), edited).expect("edit the document");

    release.send(()).expect("the install waits");
    server.ctx.loading().wait(WAIT, || false);

    let deadline = Instant::now() + WAIT;
    loop {
        let title = server
            .ctx
            .project()
            .docs
            .get(rel)
            .map(|doc| doc.title.clone());
        let texts = server.ctx.extensions().get::<TextStore>().expect("texts");
        let text = texts.current(rel).expect("a text");
        if title.as_deref() == Some("Edited while loading") && text.text.as_ref() == edited {
            break;
        }
        assert!(Instant::now() < deadline, "the edit was lost: {title:?}");
        thread::sleep(Duration::from_millis(50));
    }
    server.stop().expect("stop");
}
