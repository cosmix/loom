//! Loopback sockets with shrunk kernel buffers, so a client that stops
//! reading is visible to the bridge after a few tens of kilobytes.
use std::net::{Ipv4Addr, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use nix::fcntl::{fcntl, FcntlArg, FdFlag};

pub(super) fn shrink_buffers(socket: &impl AsRawFd, bytes: libc::c_int) {
    for option in [libc::SO_SNDBUF, libc::SO_RCVBUF] {
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
}

/// Connect to `port` on loopback with both buffers shrunk before the
/// handshake rather than after it.
///
/// TCP never withdraws a receive window it has advertised, and the final ACK
/// of the handshake advertises one sized by the buffer at that moment. Linux
/// opens a loopback connection with a window near 64 KiB; macOS starts from
/// a 128 KiB buffer and auto-sizes it upward sixteen loopback segments at a
/// time. A client shrunk only after `connect` keeps that window, and on macOS
/// it holds everything a stalled-client test sends it.
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
    shrink_buffers(&socket, bytes);
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
