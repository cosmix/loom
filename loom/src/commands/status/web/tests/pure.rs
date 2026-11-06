//! Pure-function dashboard tests: request-head parsing, origin checks, MIME
//! lookup, routing, and daemon-response classification. None of these open a
//! socket.

use std::sync::mpsc::TryRecvError;
use std::time::Duration;
use std::{fs::OpenOptions, os::fd::AsRawFd, path::Path};

use serial_test::serial;

use super::{parse_full, workspace};
use crate::commands::status::data::StatusData;
use crate::commands::status::web::adopt_for;
use crate::commands::status::web::broadcast::{self, DaemonStep, SUBSCRIBER_QUEUE_DEPTH};
use crate::commands::status::web::connection::{self, Route};
use crate::commands::status::web::model::{collect_snapshot, SnapshotSource, WebSnapshot};
use crate::commands::status::web::startup::wildcard_bootstrap_lines;
use crate::daemon::Response;
use crate::fs::tmux_tmpdir::{adopt_recorded_tmux_tmpdir, TmuxTmpdirAdoption};
use crate::fs::work_dir::WorkDir;
use crate::web_server::{assets, http};

fn hold_lock(path: &Path) -> std::fs::File {
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        // This is a lock file whose existence (not content) is what matters;
        // it must never be truncated out from under a concurrent holder.
        .truncate(false)
        .open(path)
        .expect("open orchestrator lock");
    assert_eq!(
        // SAFETY: `lock` is a valid, open file descriptor owned by this function
        // for the duration of the call, and `LOCK_EX | LOCK_NB` only locks or
        // fails without blocking or mutating the file.
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    lock
}

fn restore_tmpdir(value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var("TMUX_TMPDIR", value),
        None => std::env::remove_var("TMUX_TMPDIR"),
    }
}

#[path = "pure/security_headers.rs"]
mod security_headers;

#[test]
fn parse_head_reads_method_path_and_upgrade() {
    let request = b"GET /ws?x=1 HTTP/1.1\r\nHost: a\r\nUpgrade: websocket\r\nOrigin: http://127.0.0.1:7373\r\n\r\n";
    let head = http::parse_head(request)
        .expect("parse request")
        .expect("complete head");
    assert_eq!(head.method, "GET");
    assert_eq!(head.path, "/ws");
    assert_eq!(head.query.as_deref(), Some("x=1"));
    assert!(head.upgrade_websocket);
    assert_eq!(head.origin.as_deref(), Some("http://127.0.0.1:7373"));
    assert_eq!(head.host.as_deref(), Some("a"));
}

#[test]
fn parse_head_returns_none_on_partial() {
    assert!(http::parse_head(b"GET / HTTP/1.1\r\nHost: localhost\r\n")
        .expect("parse partial request")
        .is_none());
}

#[test]
fn parse_head_rejects_duplicate_gate_headers() {
    for request in [
        b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nHost: evil.example\r\n\r\n".as_slice(),
        b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://127.0.0.1\r\nOrigin: http://evil.example\r\n\r\n".as_slice(),
    ] {
        assert!(
            http::parse_head(request).is_err(),
            "a duplicated gate header must not resolve to its first value"
        );
    }
}

#[test]
fn origin_allowed_accepts_loopback_and_rejects_foreign() {
    for origin in [
        None,
        Some("http://127.0.0.1:5173"),
        Some("http://localhost:7373"),
        Some("https://localhost"),
        Some("http://LocalHost:7373"),
        Some("http://[::1]:7373"),
    ] {
        assert!(http::origin_allowed(origin), "{origin:?}");
    }
    for origin in [
        "http://evil.example",
        "http://127.0.0.1.evil.example",
        "http://[2001:db8::1]:7373",
        "null",
    ] {
        assert!(!http::origin_allowed(Some(origin)));
    }
}

#[test]
fn host_allowed_accepts_loopback_and_rejects_everything_else() {
    for host in [
        "127.0.0.1:41599",
        "127.0.0.1",
        "localhost",
        "LOCALHOST",
        "[::1]:7373",
    ] {
        assert!(http::host_allowed(Some(host)), "{host}");
    }
    for host in ["evil.example", "127.0.0.1.evil.example", "loom.test:7373"] {
        assert!(!http::host_allowed(Some(host)), "{host}");
    }
    // HTTP/1.1 mandates Host, so an absent one is a rejection.
    assert!(!http::host_allowed(None));
}

#[test]
fn mime_for_known_extensions() {
    assert_eq!(
        assets::mime_for("assets/index.js"),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(assets::mime_for("app.css"), "text/css; charset=utf-8");
    assert_eq!(assets::mime_for("font.woff2"), "font/woff2");
    assert_eq!(assets::mime_for("blob.bin"), "application/octet-stream");
}

#[test]
fn route_prefers_assets_then_api_then_spa_fallback() {
    assert_eq!(connection::route("/api/status"), Route::Api);
    assert_eq!(connection::route("/api/bogus"), Route::Missing);
    assert_eq!(connection::route("/assets/nope.js"), Route::Missing);
    assert_eq!(connection::route("/stages/anything"), Route::Spa);
}

#[test]
fn index_response_reports_a_missing_bundle() {
    let (status, _, content_type, body) = connection::index_response(None);
    assert_eq!(status, 503);
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert!(String::from_utf8_lossy(body).contains("web/dist"));
    let (status, _, content_type, body) = connection::index_response(Some(b"<div id=\"root\">"));
    assert_eq!(status, 200);
    assert_eq!(content_type, "text/html; charset=utf-8");
    assert_eq!(body, b"<div id=\"root\">");
}

#[test]
fn embedded_bundle_is_not_empty() {
    assert!(
        !assets::WEB_ASSETS.is_empty(),
        "the dashboard bundle is not embedded; run `cd web && bun install && bun run build`, then rebuild loom"
    );
}

#[test]
fn wildcard_bootstrap_lines_include_every_interface_address() {
    let lines = wildcard_bootstrap_lines(
        8123,
        "secret",
        [
            "127.0.0.1".parse::<std::net::IpAddr>().unwrap(),
            "10.20.30.40".parse().unwrap(),
            "192.168.1.25".parse().unwrap(),
        ],
    );
    assert_eq!(
        lines,
        [
            "  bootstrap from this machine at: http://127.0.0.1:8123/?token=secret",
            "  bootstrap from another machine at: http://10.20.30.40:8123/?token=secret",
            "  bootstrap from another machine at: http://192.168.1.25:8123/?token=secret",
        ]
    );
}

#[test]
fn route_rejects_traversal_before_the_spa_fallback() {
    for path in ["/../../etc/passwd", "/assets/../secret", "/.."] {
        assert_eq!(connection::route(path), Route::Missing, "{path}");
    }
}

#[test]
fn snapshot_frame_serializes_a_daemon_update() {
    let (_temp, base) = workspace();
    let work_dir = WorkDir::new(&base).expect("build work dir");
    let frame = broadcast::snapshot_frame(
        work_dir.root(),
        StatusData::default(),
        SnapshotSource::Daemon,
        false,
    )
    .expect("serialize daemon snapshot");
    let snapshot: WebSnapshot = serde_json::from_str(&frame).expect("snapshot JSON");
    assert_eq!(snapshot.source, SnapshotSource::Daemon);
}

#[test]
fn oversized_daemon_response_is_degraded() {
    let (_temp, base) = workspace();
    let work_dir = WorkDir::new(&base).expect("build work dir");
    let message = "daemon response exceeded the wire limit".to_owned();
    match broadcast::classify_response(
        work_dir.root(),
        Response::Error {
            message: message.clone(),
        },
        false,
    ) {
        DaemonStep::Degraded(actual) => assert_eq!(actual, message),
        _ => panic!("Response::Error must degrade to the file lane"),
    }
    assert!(matches!(
        broadcast::classify_response(
            work_dir.root(),
            Response::StatusUpdate {
                data: Box::new(StatusData::default()),
            },
            false,
        ),
        DaemonStep::Snapshot(_)
    ));
}

#[test]
fn file_snapshots_keep_terminal_failure_after_daemon_disconnect() {
    let (_temp, base) = workspace();
    let work = WorkDir::new(&base).unwrap();
    std::fs::write(
        work.root().join("orchestrator.complete"),
        r#"{"kind":"scheduler_failure","message":"panic"}"#,
    )
    .unwrap();
    let frame = broadcast::snapshot_frame(
        work.root(),
        StatusData::default(),
        SnapshotSource::Files,
        false,
    )
    .unwrap();
    let snapshot: WebSnapshot = serde_json::from_str(&frame).unwrap();
    assert!(snapshot
        .notice
        .unwrap()
        .contains("Scheduler failure: panic"));
}

#[test]
fn second_poll_of_an_unchanged_tree_sends_only_a_tick() {
    let (_temp, base) = workspace();
    let work_dir = WorkDir::new(&base).expect("build work dir");
    let broadcaster = broadcast::Broadcaster::new();
    let receiver = broadcaster.subscribe();
    broadcast::poll_files_once(&broadcaster, &work_dir, work_dir.root(), None, false)
        .expect("first file poll");
    broadcast::poll_files_once(&broadcaster, &work_dir, work_dir.root(), None, false)
        .expect("second file poll");
    let first = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("first frame");
    let second = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("second frame");
    assert!(first.starts_with(r#"{"t":"full""#), "{first:.80}");
    let tick: serde_json::Value = serde_json::from_str(&second).expect("tick JSON");
    assert_eq!(tick["t"], "tick");
    assert_eq!(tick["stages"], serde_json::json!({}));
    assert!(receiver.recv_timeout(Duration::from_millis(150)).is_err());
}

#[test]
fn poll_files_once_carries_the_degrade_notice() {
    let (_temp, base) = workspace();
    let work_dir = WorkDir::new(&base).expect("build work dir");
    let broadcaster = broadcast::Broadcaster::new();
    let receiver = broadcaster.subscribe();
    broadcast::poll_files_once(
        &broadcaster,
        &work_dir,
        work_dir.root(),
        Some("daemon lane degraded"),
        false,
    )
    .expect("file poll carrying a notice");
    let frame = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("degraded frame");
    let snapshot = parse_full(&frame);
    assert_eq!(snapshot.notice.as_deref(), Some("daemon lane degraded"));
    // Pins the real publish path (not just the fixture helper) to the
    // build's tag-derived version.
    assert_eq!(snapshot.version, crate::version::VERSION);
}

#[test]
fn a_stalled_subscriber_is_dropped_once_its_queue_fills() {
    let (_temp, base) = workspace();
    let work_dir = WorkDir::new(&base).expect("build work dir");
    let snapshot = collect_snapshot(
        work_dir.root(),
        StatusData::default(),
        SnapshotSource::Files,
        false,
    );
    let broadcaster = broadcast::Broadcaster::new();
    let receiver = broadcaster.subscribe();
    // The first publish is a `full`, every later one a lone `tick`: one
    // message each, so the queue fills after exactly its depth in publishes.
    for index in 0..SUBSCRIBER_QUEUE_DEPTH + 4 {
        let mut next = snapshot.clone();
        next.generated_at += chrono::Duration::seconds(i64::try_from(index).unwrap());
        broadcaster.publish(next);
    }
    let mut delivered = 0;
    while receiver.try_recv().is_ok() {
        delivered += 1;
    }
    assert_eq!(delivered, SUBSCRIBER_QUEUE_DEPTH);
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
#[serial]
fn adopt_is_called_with_the_state_dir() {
    let (_temp, base) = workspace();
    let work_dir = WorkDir::new(&base).expect("build work dir");
    let state_dir = work_dir.root();
    std::fs::write(state_dir.join("tmux-tmpdir"), "/daemon/tmux").expect("record tmpdir");
    let lock = hold_lock(&state_dir.join("orchestrator.lock"));
    let ambient = std::env::var_os("TMUX_TMPDIR");
    std::env::set_var("TMUX_TMPDIR", "/shell/tmux");
    assert!(matches!(
        adopt_for(&work_dir),
        TmuxTmpdirAdoption::Adopted { .. }
    ));
    assert_eq!(
        adopt_recorded_tmux_tmpdir(&base),
        TmuxTmpdirAdoption::DaemonNotRunning
    );
    drop(lock);
    restore_tmpdir(ambient);
}
