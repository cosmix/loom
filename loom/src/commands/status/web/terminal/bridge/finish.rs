//! How a bridge ends: which close frame the browser gets, then the child is reaped.

use std::net::TcpStream;

use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::CloseFrame;
use tungstenite::{Message, WebSocket};

use super::super::protocol::{CLOSE_ENDED, CLOSE_REFUSED, CLOSE_SERVER_STOPPING, CLOSE_TOO_LARGE};
use super::super::pty::PtyChild;

pub(super) enum Stop {
    Ended,
    ServerStopping,
    Peer,
    TooLarge,
    Stalled,
    OutputStalled,
}

pub(super) fn finish(mut socket: WebSocket<TcpStream>, child: PtyChild, stop: Stop) {
    let frame = match stop {
        Stop::Ended => Some(CloseFrame {
            code: CloseCode::from(CLOSE_ENDED),
            reason: "ended".into(),
        }),
        Stop::ServerStopping => Some(CloseFrame {
            code: CloseCode::from(CLOSE_SERVER_STOPPING),
            reason: "server stopping".into(),
        }),
        Stop::TooLarge => Some(CloseFrame {
            code: CloseCode::from(CLOSE_TOO_LARGE),
            reason: "message too large".into(),
        }),
        // 4008 rather than a bare close: the page already reports that code as
        // a refusal carrying the server's reason, where `Stop::Peer`'s absent
        // frame reads as tmux having exited normally.
        Stop::Stalled => Some(CloseFrame {
            code: CloseCode::from(CLOSE_REFUSED),
            reason: "input stalled".into(),
        }),
        // The mirror of `Stalled` for the output direction: a peer that is
        // connected but no longer drains what the terminal prints.
        Stop::OutputStalled => Some(CloseFrame {
            code: CloseCode::from(CLOSE_REFUSED),
            reason: "output stalled".into(),
        }),
        Stop::Peer => None,
    };
    if let Some(frame) = frame {
        let _ = socket.send(Message::Close(Some(frame)));
    }
    let _ = socket.close(None);
    let _ = socket.flush();
    child.shutdown();
}
