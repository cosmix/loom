//! Contract of stage runtime-and-dashboard-server: a terminal peer that stops
//! reading is disconnected, and its terminal slot is freed.

use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;

use crate::commands::status::web::MAX_INBOUND_BYTES;
use crate::process::sandbox_probe::{
    loopback_bindable, skip_unless, skip_without_group_signals, skip_without_pty,
};
use crate::web_server::limits::{acquire_terminal_slot, Limits};

use super::bridge::{self, Timing};
use super::protocol::{Mode, WindowSize};
use super::pty::PtyChild;

const TEST: &str = "a_peer_that_never_reads_is_disconnected_and_its_slot_freed";

/// Serves one bridge over a PTY running `yes`, holding one terminal slot of
/// `limits` until the bridge returns.
fn spawn_bridge(
    listener: TcpListener,
    limits: Arc<Limits>,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let _slot = acquire_terminal_slot(&limits).expect("terminal slot");
        let (stream, _) = listener.accept().expect("accept bridge client");
        let socket = tungstenite::accept(stream).expect("accept WebSocket");
        let mut command = Command::new("sh");
        command.args(["-c", "yes"]);
        let child = PtyChild::spawn(&mut command, WindowSize { cols: 80, rows: 24 })
            .expect("spawn PTY child");
        let timing = Timing {
            stall: Duration::from_secs(1),
            ..Timing::DEFAULT
        };
        bridge::run_with(socket, child, Mode::Control, running.as_ref(), timing);
    })
}

#[test]
fn a_peer_that_never_reads_is_disconnected_and_its_slot_freed() {
    if skip_unless(
        loopback_bindable(),
        TEST,
        "loopback sockets are unavailable",
    ) || skip_without_pty(TEST)
        || skip_without_group_signals(TEST)
    {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind bridge listener");
    let port = listener.local_addr().expect("listener address").port();
    let limits = Limits::new(MAX_INBOUND_BYTES);
    let running = Arc::new(AtomicBool::new(true));
    let server = spawn_bridge(listener, Arc::clone(&limits), Arc::clone(&running));
    let request = format!("ws://127.0.0.1:{port}/")
        .into_client_request()
        .expect("build WebSocket request");
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect bridge client");
    let (_client, _) = tungstenite::client(request, stream).expect("handshake");
    assert_eq!(limits.terminal_count(), 1, "the bridge holds one slot");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !server.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    let returned = server.is_finished();
    running.store(false, Ordering::SeqCst);
    assert!(
        returned,
        "the bridge kept a peer that never reads past 15 s"
    );
    server.join().expect("bridge thread");
    assert_eq!(limits.terminal_count(), 0, "the slot is freed");
}
