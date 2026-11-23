//! Contract of stage runtime-and-dashboard-server: an authenticated attach
//! past the terminal limit completes the handshake and closes 4011.

use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::CloseFrame;
use tungstenite::Message;

use crate::commands::status::web::tests::{skip_without_loopback, start_with, stop, workspace};
use crate::web_server::limits::{acquire_terminal_slot, MAX_TERMINALS};

use super::tests_upgrade::{connect_terminal, terminal_options};

const TEST: &str = "an_over_limit_authenticated_attach_closes_4011_after_the_handshake";

fn close_frame(message: tungstenite::Result<Message>) -> CloseFrame {
    match message {
        Ok(Message::Close(Some(frame))) => frame,
        other => panic!("expected a close frame, got {other:?}"),
    }
}

#[test]
fn an_over_limit_authenticated_attach_closes_4011_after_the_handshake() {
    if skip_without_loopback(TEST) {
        return;
    }
    let (_temp, base) = workspace();
    let (port, running, limits) = start_with(base, terminal_options());
    let token = "a".repeat(64);
    let held = (0..MAX_TERMINALS)
        .map(|_| acquire_terminal_slot(&limits).expect("terminal slot"))
        .collect::<Vec<_>>();
    let busy = close_frame(connect_terminal(port, &token).read());
    drop(held);
    let unknown = close_frame(connect_terminal(port, &token).read());
    stop(running);
    assert_eq!(busy.code, CloseCode::Library(4011), "{busy:?}");
    assert_eq!(busy.reason.as_str(), "terminal limit reached");
    assert_eq!(unknown.code, CloseCode::Library(4004), "{unknown:?}");
}
