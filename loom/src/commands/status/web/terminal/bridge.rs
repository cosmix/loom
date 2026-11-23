use std::collections::VecDeque;
use std::net::TcpStream;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::poll::PollFlags;
use tungstenite::error::CapacityError;
use tungstenite::{Error, Message, WebSocket};

mod finish;
mod liveness;
mod poll;

use finish::{finish, Stop};
use liveness::Liveness;
pub(super) use liveness::Timing;
use poll::{poll_ready, socket_gone};

use super::protocol::{clamp, parse_client_frame, scroll_input, ClientFrame, Mode};
use super::pty::PtyChild;
use crate::commands::status::web::MAX_INBOUND_BYTES;

const READ_TIMEOUT: Duration = Duration::from_millis(5);
const WRITE_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_PENDING_INPUT: usize = 64 * 1024;
/// How long the input queue may sit at `MAX_PENDING_INPUT` before the bridge
/// gives up on the child, and how long an output flush may make no progress
/// before it gives up on the browser.
///
/// A gated bridge holds a terminal slot, a PTY and a tmux client, and neither
/// `poll` nor a socket read can tell whether the browser is still there: the
/// mask is narrowed, a FIN raises no POLLHUP, and while gated the unread
/// backlog is returned ahead of any EOF behind it. A wedged child never sends
/// a FIN in the first place. So the hold is bounded by elapsed time rather
/// than by a readiness flag, which also keeps one mechanism across Linux and
/// macOS. A live tmux client drains 64 KiB in milliseconds, so surviving this
/// only needs the PTY to accept roughly 2 KiB a second.
const GATE_STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Messages taken from the browser in one poll iteration. The
/// `MAX_PENDING_INPUT` gate already bounds a keystroke flood, but resize
/// frames and view-mode input never reach `pending`, so on their own they
/// would let a client hold the iteration open for as long as it keeps the
/// socket readable, starving PTY output and the `running` re-check.
const MAX_INBOUND_MESSAGES: usize = 64;
/// Ceiling on tungstenite's outbound buffer. A write that timed out is
/// retried rather than fatal (see `outbound_stop`), so this is what ends the
/// bridge when a client never resumes reading at all.
const MAX_WRITE_BUFFER: usize = 2 * 1024 * 1024;

/// Pump bytes between an accepted WebSocket and a PTY child until either side ends.
/// Consumes both. Returns when the socket is closed and the child is reaped.
pub(super) fn run(
    socket: tungstenite::WebSocket<TcpStream>,
    child: PtyChild,
    mode: Mode,
    running: &AtomicBool,
) {
    run_with(socket, child, mode, running, Timing::DEFAULT);
}

/// `run` with the give-up deadlines supplied rather than taken from
/// `Timing::DEFAULT`. Exists for the tests: covering these paths otherwise
/// costs tens of seconds of wall clock per assertion.
pub(super) fn run_with(
    mut socket: tungstenite::WebSocket<TcpStream>,
    child: PtyChild,
    mode: Mode,
    running: &AtomicBool,
    timing: Timing,
) {
    if !configure(&mut socket) {
        finish(socket, child, Stop::Peer);
        return;
    }
    let stop = pump_loop(&mut socket, &child, mode, running, timing);
    finish(socket, child, stop);
}

fn pump_loop(
    socket: &mut WebSocket<TcpStream>,
    child: &PtyChild,
    mode: Mode,
    running: &AtomicBool,
    timing: Timing,
) -> Stop {
    let tcp_fd = socket.get_ref().as_raw_fd();
    let mut pending = VecDeque::new();
    let mut buffered = false;
    let mut live = Liveness::new(timing, Instant::now());
    loop {
        if !running.load(Ordering::SeqCst) {
            return Stop::ServerStopping;
        }
        let gate_shut = pending.len() >= MAX_PENDING_INPUT;
        if live.gate_stalled(Instant::now(), gate_shut) {
            return Stop::Stalled;
        }
        let ready = match poll_ready(child.master(), tcp_fd, pending.len()) {
            Ok(ready) => ready,
            Err(Errno::EINTR) => continue,
            Err(_) => return Stop::Peer,
        };
        let stalled = match pump_pty(socket, child, ready.master) {
            Ok(stalled) => stalled,
            Err(stop) => return stop,
        };
        if live.output_stalled(Instant::now(), stalled) {
            return Stop::OutputStalled;
        }
        if (ready.tcp.contains(PollFlags::POLLIN) || buffered) && !gate_shut {
            let read = pump_socket(socket, child, mode, &mut pending, &mut buffered, &mut live);
            if let Some(stop) = read {
                return stop;
            }
        }
        if ready.master.contains(PollFlags::POLLOUT) && !drain_pending(child, &mut pending) {
            return Stop::Peer;
        }
        if socket_gone(ready.tcp) {
            return Stop::Peer;
        }
        let gate_shut = pending.len() >= MAX_PENDING_INPUT;
        if let Some(stop) = keepalive(socket, &mut live, stalled, gate_shut) {
            return stop;
        }
    }
}

fn configure(socket: &mut WebSocket<TcpStream>) -> bool {
    socket.set_config(|config| {
        config.max_message_size = Some(MAX_INBOUND_BYTES);
        config.max_frame_size = Some(MAX_INBOUND_BYTES);
        config.max_write_buffer_size = MAX_WRITE_BUFFER;
    });
    let peer = socket.get_mut();
    peer.set_read_timeout(Some(READ_TIMEOUT)).is_ok()
        && peer.set_write_timeout(Some(WRITE_TIMEOUT)).is_ok()
}

/// One pass over the PTY side. `Ok(true)` means this pass's flush or send hit
/// a stall (the peer is not taking output), `Ok(false)` that it completed or
/// had nothing to do.
fn pump_pty(
    socket: &mut WebSocket<TcpStream>,
    child: &PtyChild,
    events: PollFlags,
) -> Result<bool, Stop> {
    // Finish a frame the client's receive window left half-written. Costs
    // nothing while the out-buffer is empty, and is the only thing that
    // resumes a stalled write once the PTY itself has gone quiet. A stall ends
    // the pass instead of falling through to the read: `send` below is a write
    // plus a flush, so carrying on would park on the same drain twice in one
    // iteration - unbounded, since `WRITE_TIMEOUT` is per `write(2)` and a
    // client taking a trickle never trips it - and would keep pulling 16 KiB a
    // pass off the master into the out-buffer, discarding tmux's own
    // backpressure to buffer toward `MAX_WRITE_BUFFER` here instead.
    if let Err(error) = socket.flush() {
        return outbound(Err(error));
    }
    if events.contains(PollFlags::POLLIN) {
        let mut bytes = [0u8; 16 * 1024];
        return match child.read(&mut bytes) {
            Ok(Some(0)) => Err(Stop::Ended),
            Ok(Some(count)) => outbound(socket.send(Message::binary(bytes[..count].to_vec()))),
            Ok(None) => Ok(false),
            Err(_) => Err(Stop::Peer),
        };
    }
    if events.contains(PollFlags::POLLHUP) {
        Err(Stop::Ended)
    } else if events.intersects(PollFlags::POLLERR | PollFlags::POLLNVAL) {
        Err(Stop::Peer)
    } else {
        Ok(false)
    }
}

fn pump_socket(
    socket: &mut WebSocket<TcpStream>,
    child: &PtyChild,
    mode: Mode,
    pending: &mut VecDeque<u8>,
    buffered: &mut bool,
    live: &mut Liveness,
) -> Option<Stop> {
    // `buffered` stands in for readiness `poll` cannot report: whole messages
    // left in the codec's own read buffer by a gated pass are invisible at the
    // fd, so waiting for POLLIN would strand them.
    //
    // Assume the codec still holds messages until a read says otherwise. Only
    // the `WouldBlock` arm below proves the socket and the codec's buffer are
    // both empty; every other way out of this pass is a gate, and the caller
    // has to come back for the rest without waiting on `poll`.
    *buffered = true;
    for _ in 0..MAX_INBOUND_MESSAGES {
        // Re-checked per message, not only before the call: one invocation
        // otherwise drains a receive buffer that already holds several
        // messages, so the queue peaks at that buffered total instead of at
        // the cap. With the check here the peak is one message past the cap.
        if pending.len() >= MAX_PENDING_INPUT {
            return None;
        }
        match socket.read() {
            Ok(Message::Close(_)) | Err(Error::ConnectionClosed | Error::AlreadyClosed) => {
                return Some(Stop::Peer);
            }
            // Any frame at all, a pong included, shows the peer is still there.
            Ok(message) => {
                live.heard(Instant::now());
                apply_client_frame(child, mode, pending, &message);
            }
            // A paste past `MAX_INBOUND_BYTES`. Named on the wire rather than
            // folded into the bare close a dead peer gets, which the browser
            // cannot tell from a dropped connection.
            Err(Error::Capacity(CapacityError::MessageTooLong { .. })) => {
                return Some(Stop::TooLarge);
            }
            Err(Error::Io(error)) if stalled(&error) => {
                *buffered = false;
                return None;
            }
            Err(_) => return Some(Stop::Peer),
        }
    }
    None
}

/// Act on one browser message. A viewer's keystrokes and resizes are dropped:
/// the PTY keeps the size it started with, so a viewer cannot reflow what a
/// controller is looking at.
fn apply_client_frame(child: &PtyChild, mode: Mode, pending: &mut VecDeque<u8>, message: &Message) {
    match parse_client_frame(message) {
        Some(ClientFrame::Input(bytes)) if mode == Mode::Control => pending.extend(bytes),
        Some(ClientFrame::Resize(size)) if mode == Mode::Control => {
            let _ = child.resize(clamp(size));
        }
        Some(ClientFrame::Scroll(pages)) => pending.extend(scroll_input(pages)),
        Some(ClientFrame::Input(_) | ClientFrame::Resize(_)) | None => {}
    }
}

/// End the bridge for a peer that has said nothing for too long, otherwise
/// ping on the keepalive interval. A ping is never queued behind a stalled
/// frame, or a stuck peer would accumulate them.
fn keepalive(
    socket: &mut WebSocket<TcpStream>,
    live: &mut Liveness,
    stalled: bool,
    gate_shut: bool,
) -> Option<Stop> {
    let now = Instant::now();
    if live.silent(now, gate_shut) {
        return Some(Stop::Peer);
    }
    if !stalled && live.ping_due(now) {
        return outbound_stop(socket.send(Message::Ping(Default::default())));
    }
    None
}

/// `outbound_stop`, plus whether the write was a tolerated stall.
fn outbound(result: tungstenite::Result<()>) -> Result<bool, Stop> {
    let stalled_now = matches!(&result, Err(Error::Io(error)) if stalled(error));
    match outbound_stop(result) {
        Some(stop) => Err(stop),
        None => Ok(stalled_now),
    }
}

/// Decide whether an outbound write ends the bridge.
///
/// A client that has stopped reading is not a dead one: `WRITE_TIMEOUT` expiry
/// leaves the frame in tungstenite's out-buffer for a later flush to resume,
/// so nothing is lost by retrying on the next iteration. Every other error
/// stays fatal, `Error::WriteBufferFull` above all - it returns immediately
/// instead of parking, so tolerating it would spin a core, and it is the
/// bound (`MAX_WRITE_BUFFER`) that terminates a stall that never clears.
fn outbound_stop(result: tungstenite::Result<()>) -> Option<Stop> {
    match result {
        Ok(()) => None,
        Err(Error::Io(error)) if stalled(&error) => None,
        Err(_) => Some(Stop::Peer),
    }
}

/// A transfer that has not happened yet, as opposed to one that failed.
///
/// `Interrupted` belongs here: a socket call with `SO_SNDTIMEO`/`SO_RCVTIMEO`
/// set is never restarted after a signal handler runs, `SA_RESTART` or not
/// (signal(7)), so any handler in the process (the server's own `ctrlc`
/// one) turns a delivered signal into `EINTR` on whichever bridge was mid-call.
fn stalled(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    )
}

fn drain_pending(child: &PtyChild, pending: &mut VecDeque<u8>) -> bool {
    while !pending.is_empty() {
        let result = {
            let (front, back) = pending.as_slices();
            child.write_some(if front.is_empty() { back } else { front })
        };
        match result {
            Ok(0) => return true,
            Ok(count) => {
                drop(pending.drain(..count));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return true,
            Err(_) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn a_signal_interrupting_a_transfer_is_retried_not_read_as_a_dead_peer() {
        for kind in [
            ErrorKind::Interrupted,
            ErrorKind::WouldBlock,
            ErrorKind::TimedOut,
        ] {
            assert!(stalled(&IoError::from(kind)), "{kind:?}");
            assert!(outbound_stop(Err(Error::Io(IoError::from(kind)))).is_none());
            assert!(matches!(
                outbound(Err(Error::Io(IoError::from(kind)))),
                Ok(true)
            ));
        }
        assert!(matches!(outbound(Ok(())), Ok(false)));
        let reset = Err(Error::Io(IoError::from(ErrorKind::ConnectionReset)));
        assert!(matches!(outbound_stop(reset), Some(Stop::Peer)));
    }
}
