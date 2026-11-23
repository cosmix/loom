//! Readiness of the PTY master and the browser socket for one bridge pass.

use std::os::fd::{BorrowedFd, RawFd};

use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

use super::MAX_PENDING_INPUT;

const POLL_TIMEOUT_MS: u16 = 50;

pub(super) struct Ready {
    pub(super) master: PollFlags,
    pub(super) tcp: PollFlags,
}

/// Whether `poll` reported the socket as gone rather than merely not ready.
/// These three arrive whether or not they were asked for, which is what lets
/// the gated event mask narrow to nothing without losing them.
pub(super) fn socket_gone(events: PollFlags) -> bool {
    events.intersects(PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL)
}

pub(super) fn poll_ready(
    master: BorrowedFd<'_>,
    tcp_fd: RawFd,
    pending: usize,
) -> nix::Result<Ready> {
    let master_events = if pending > 0 {
        PollFlags::POLLIN | PollFlags::POLLOUT
    } else {
        PollFlags::POLLIN
    };
    // Backpressure: with the queue full, stop asking whether the socket is
    // readable, so the browser stalls against TCP's own receive window instead
    // of us buffering without bound. The gate belongs on the event mask, not
    // on the read call alone: skipping the read while still asking for POLLIN
    // makes `poll` return immediately every iteration and burns a core until
    // the client moves. What the gate costs is peer-close detection: a closing
    // tab sends a FIN, which raises POLLIN and, on Linux, POLLRDHUP - never
    // the POLLHUP that only a reset or a full shutdown produces. So the hangup
    // check in `run` cannot see a half-closed browser here. Reading the socket
    // cannot see one either while gated, since the unread backlog is the
    // backpressure and a peek returns that backlog rather than the EOF behind
    // it; `GATE_STALL_TIMEOUT` bounds the hold instead.
    let tcp_events = if pending < MAX_PENDING_INPUT {
        PollFlags::POLLIN
    } else {
        PollFlags::empty()
    };
    // Safety: `tcp_fd` remains owned by `socket` for the full bridge loop.
    let tcp = unsafe { BorrowedFd::borrow_raw(tcp_fd) };
    let mut fds = [
        PollFd::new(master, master_events),
        PollFd::new(tcp, tcp_events),
    ];
    poll(&mut fds, PollTimeout::from(POLL_TIMEOUT_MS))?;
    Ok(Ready {
        master: fds[0].revents().unwrap_or(PollFlags::empty()),
        tcp: fds[1].revents().unwrap_or(PollFlags::empty()),
    })
}
