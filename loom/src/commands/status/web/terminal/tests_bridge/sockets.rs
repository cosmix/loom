//! Loopback sockets with shrunk kernel buffers, so a client that stops
//! reading is visible to the bridge after a few tens of kilobytes.
use std::net::{Ipv4Addr, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use nix::fcntl::{fcntl, FcntlArg, FdFlag};

/// Send buffer of the client socket in `connect_shrunk`: small enough to cap
/// its macOS receive buffer (see there), large enough for the WebSocket
/// handshake request of about 170 bytes.
const CLIENT_SNDBUF: libc::c_int = 1024;

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

/// Connect to `port` on loopback with the client's buffers set before the
/// handshake rather than after it: `SO_RCVBUF` to `bytes`, `SO_SNDBUF` to
/// `CLIENT_SNDBUF`.
///
/// On macOS the receive buffer grows past a set `SO_RCVBUF`, in steps of
/// `16 * t_maxseg`, and `t_maxseg` follows the client's send buffer at connect
/// time. A 1 KiB send buffer caps the receive buffer near 47 KiB; 8 KiB allows
/// about 264 KiB, enough to absorb a whole stalled-client test.
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
    set_buffer(&socket, libc::SO_SNDBUF, CLIENT_SNDBUF);
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
