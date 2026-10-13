//! Loopback sockets with shrunk kernel buffers, so a client that stops
//! reading stalls the bridge after a bounded amount of output, and a
//! measurement of that amount on this host.
use std::io::{ErrorKind, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

use nix::fcntl::{fcntl, FcntlArg, FdFlag};

fn set_buffer(socket: &impl AsRawFd, option: libc::c_int, bytes: libc::c_int) {
    // SAFETY: `socket` owns a live socket fd for the duration of the call,
    // and the value pointer and length describe one complete `c_int`.
    let rc = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            option,
            (&raw const bytes).cast::<libc::c_void>(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    // Checked, not discarded: a failure here leaves the autotuned
    // megabyte-scale buffers in place, and every caller's stalled client
    // would then be absorbed by the network and test nothing at all.
    assert_ne!(
        rc,
        -1,
        "shrink socket buffer option {option}: {}",
        std::io::Error::last_os_error()
    );
}

pub(super) fn shrink_buffers(socket: &impl AsRawFd, bytes: libc::c_int) {
    for option in [libc::SO_SNDBUF, libc::SO_RCVBUF] {
        set_buffer(socket, option, bytes);
    }
}

/// Connect to `port` on loopback with `SO_RCVBUF` set to `bytes` before
/// `connect`. On macOS that stops the receive buffer autosizing, but it still
/// settles at 20 times the 16 KiB loopback MSS, about 320 KiB, and no socket
/// option lowers it; on Linux the set size holds, at tens of KiB.
pub(super) fn connect_shrunk(port: u16, bytes: libc::c_int) -> TcpStream {
    // SAFETY: `socket(2)` takes no pointers; the result is checked below.
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
    assert_ne!(fd, -1, "create client socket");
    // SAFETY: `fd` is a freshly created socket that nothing else owns.
    let socket = unsafe { OwnedFd::from_raw_fd(fd) };
    // `SOCK_CLOEXEC` does not exist on macOS, so set close-on-exec with fcntl
    // as `PtyChild::spawn` does; otherwise a PTY child spawned by the test
    // inherits this socket and keeps the connection open after the test drops it.
    fcntl(&socket, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).expect("set FD_CLOEXEC on client socket");
    set_buffer(&socket, libc::SO_RCVBUF, bytes);
    // SAFETY: all-zero bytes are a valid `sockaddr_in`; the BSDs' `sin_len`
    // may stay zero, as the kernel takes the length from `connect`'s argument.
    let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    addr.sin_family = libc::AF_INET as libc::sa_family_t;
    addr.sin_port = port.to_be();
    addr.sin_addr.s_addr = u32::from(Ipv4Addr::LOCALHOST).to_be();
    // SAFETY: `addr` is a complete `sockaddr_in` that outlives the call, and
    // the length passed is exactly its size.
    let rc = unsafe {
        libc::connect(
            socket.as_raw_fd(),
            (&raw const addr).cast::<libc::sockaddr>(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    };
    assert_ne!(
        rc,
        -1,
        "connect bridge client: {}",
        std::io::Error::last_os_error()
    );
    TcpStream::from(socket)
}

/// How many bytes a client that never reads absorbs on this host, with the
/// buffers `start_bridge_with` gives both ends for `bytes`: the client's
/// receive queue plus the server's send buffer. macOS opens a stalled client's
/// window gradually, about 100 KB per half second up to its full ~320 KiB, so
/// a short pause reads as full far too early (a 50 ms one stopped at 128 KiB).
/// The count ends only when a write moves nothing for `CAPACITY_QUIET`.
pub(super) fn stalled_client_capacity(bytes: libc::c_int) -> usize {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind capacity listener");
    let port = listener
        .local_addr()
        .expect("capacity listener address")
        .port();
    // Never read, and held open until the count is done: dropping it would
    // close the connection under the writes below.
    let _client = connect_shrunk(port, bytes);
    let (mut server, _) = listener.accept().expect("accept capacity client");
    shrink_buffers(&server, bytes);
    server
        .set_write_timeout(Some(CAPACITY_QUIET))
        .expect("set capacity server write timeout");
    let chunk = [b'x'; 16 * 1024];
    let mut total = 0;
    loop {
        match server.write(&chunk) {
            Ok(written) => total += written,
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return total;
            }
            Err(err) => panic!("fill the capacity client: {err}"),
        }
    }
}

/// How long a write must move nothing before the stalled client counts as
/// full: twice the bridge's own write timeout.
const CAPACITY_QUIET: Duration = Duration::from_millis(500);
