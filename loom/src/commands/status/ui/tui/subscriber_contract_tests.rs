//! Contract: a subscriber socket that the daemon closes while the daemon is
//! still alive is replaced by a fresh subscription, and the session goes on.
//!
//! Seams, visible to the `tui` module:
//! - `daemon_client::subscribe(&mut UnixStream, &Path) -> Result<()>`: the
//!   second argument is the work directory the stream is connected to;
//! - `TuiApp::headless()`, `TuiApp::advance(..) -> Result<bool>` (`true`
//!   keeps the session) and `TuiApp::draw_view(..)`.
//!
//! The fake daemon accepts twice. The first subscription is closed at once;
//! the second receives one status update and stays open until the test ends.

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ratatui::{backend::TestBackend, Terminal};
use tempfile::TempDir;

use super::app::TuiApp;
use super::daemon_client::{connect, subscribe};
use crate::daemon::{
    read_message, socket_path, write_message, Request, Response, ResponseFrameReader,
};
use crate::process::sandbox_probe::skip_without_unix_socket;
use crate::status::data::{StageSummary, StatusData};
use crate::user_config::redirect_user_config;

const WAIT: Duration = Duration::from_secs(5);

/// Answer Ping and SubscribeStatus the way the daemon does; the returned
/// stream is the subscribed connection.
fn accept_subscriber(listener: &UnixListener) -> UnixStream {
    let (mut stream, _) = listener.accept().expect("accept a client");
    stream.set_read_timeout(Some(WAIT)).unwrap();
    loop {
        match read_message::<Request, _>(&mut stream).expect("a client request") {
            Request::Ping { .. } => write_message(&mut stream, &Response::Pong).unwrap(),
            Request::SubscribeStatus { .. } => {
                write_message(&mut stream, &Response::Ok).unwrap();
                return stream;
            }
            other => panic!("unexpected request from the TUI: {other:?}"),
        }
    }
}

fn probe_status() -> Response {
    let stage = StageSummary {
        id: "probe-stage".into(),
        name: "Probe Stage".into(),
        ..StageSummary::default()
    };
    let data = StatusData {
        stages: vec![stage],
        ..StatusData::default()
    };
    Response::StatusUpdate {
        data: Box::new(data),
    }
}

/// Serves two subscriptions: drops the first (signalling `closed`), sends
/// one update on the second (signalling `sent`) and holds it until `release`
/// is dropped.
fn fake_daemon(
    listener: UnixListener,
    closed: Sender<()>,
    sent: Sender<()>,
    release: Receiver<()>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        drop(accept_subscriber(&listener));
        closed.send(()).unwrap();
        let mut second = accept_subscriber(&listener);
        write_message(&mut second, &probe_status()).unwrap();
        sent.send(()).unwrap();
        let _ = release.recv();
    })
}

fn screen(terminal: &mut Terminal<TestBackend>, app: &mut TuiApp) -> String {
    terminal.draw(|frame| app.draw_view(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_subscription(work_dir: &Path) -> UnixStream {
    let mut stream = connect(work_dir).expect("connect to the fake daemon");
    subscribe(&mut stream, work_dir).expect("subscribe to the fake daemon");
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    stream
}

#[test]
fn eof_with_a_live_daemon_reconnects() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let socket = socket_path(root.path());
    if skip_without_unix_socket("eof_with_a_live_daemon_reconnects", &socket) {
        return;
    }
    let listener = UnixListener::bind(&socket).unwrap();
    let (closed_tx, closed_rx) = channel();
    let (sent_tx, sent_rx) = channel();
    let (release_tx, release_rx) = channel();
    let daemon = fake_daemon(listener, closed_tx, sent_tx, release_rx);

    let mut stream = first_subscription(root.path());
    closed_rx
        .recv_timeout(WAIT)
        .expect("the first socket closes");

    let mut app = TuiApp::headless();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
    assert!(
        app.advance(&mut stream, root.path(), &mut reader).unwrap(),
        "the session ended at EOF although the daemon still listens"
    );
    sent_rx.recv_timeout(WAIT).expect("the TUI resubscribed");
    assert!(
        app.advance(&mut stream, root.path(), &mut reader).unwrap(),
        "the session ended instead of reading the new subscription"
    );

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let shown = screen(&mut terminal, &mut app);
    assert!(
        shown.contains("Probe Stage") || shown.contains("probe-stage"),
        "the update from the new subscription is not shown:\n{shown}"
    );
    assert!(
        !shown.contains("Daemon exited"),
        "a live daemon is reported as exited:\n{shown}"
    );

    drop(release_tx);
    daemon.join().unwrap();
}
