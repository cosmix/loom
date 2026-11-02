//! One absolute deadline for connecting, writing, and reading a daemon RPC.

use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct DeadlineStream {
    stream: UnixStream,
    deadline: Instant,
}

impl DeadlineStream {
    #[cfg(test)]
    pub(super) fn connected_for_test(stream: UnixStream, timeout: Duration) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            deadline: Instant::now() + timeout,
        })
    }

    pub(super) fn connect(path: &Path, timeout: Duration) -> io::Result<Self> {
        let deadline = Instant::now() + timeout;
        let address = socket_address(path)?;
        loop {
            let stream = UnixStream::from(nonblocking_socket()?);
            let transport = Self { stream, deadline };
            transport.remaining()?;
            if transport.connect_once(&address)? {
                return Ok(transport);
            }
            // A failed connect leaves the descriptor's state unspecified.
            // Drop it before the next attempt, retaining the original deadline.
            let pause = transport.remaining()?.min(Duration::from_millis(1));
            drop(transport);
            std::thread::sleep(pause);
        }
    }

    fn connect_once(&self, address: &libc::sockaddr_un) -> io::Result<bool> {
        // SAFETY: address is initialized and this descriptor is an AF_UNIX socket.
        let result = unsafe {
            libc::connect(
                self.stream.as_raw_fd(),
                (&raw const *address).cast::<libc::sockaddr>(),
                std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
            )
        };
        if result == 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EISCONN) => Ok(true),
            Some(libc::EINTR) => Ok(false),
            Some(libc::EAGAIN) => Ok(false),
            Some(libc::EINPROGRESS | libc::EALREADY) => {
                self.wait(libc::POLLOUT)?;
                self.finish_connect()?;
                match self.stream.peer_addr() {
                    Ok(_) => Ok(true),
                    Err(error) if error.kind() == io::ErrorKind::NotConnected => Ok(false),
                    Err(error) => Err(error),
                }
            }
            _ => Err(error),
        }
    }

    fn finish_connect(&self) -> io::Result<()> {
        let mut socket_error: libc::c_int = 0;
        let mut length = std::mem::size_of_val(&socket_error) as libc::socklen_t;
        // SAFETY: getsockopt writes at most length bytes into socket_error.
        let result = unsafe {
            libc::getsockopt(
                self.stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&raw mut socket_error).cast(),
                &mut length,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        if socket_error != 0 {
            return Err(io::Error::from_raw_os_error(socket_error));
        }
        Ok(())
    }

    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "daemon exchange deadline elapsed")
            })
    }

    fn wait(&self, events: libc::c_short) -> io::Result<()> {
        loop {
            let remaining = self.remaining()?;
            let millis = remaining.as_millis().clamp(1, i32::MAX as u128) as libc::c_int;
            let mut descriptor = libc::pollfd {
                fd: self.stream.as_raw_fd(),
                events,
                revents: 0,
            };
            // SAFETY: descriptor is a valid one-element pollfd array.
            let result = unsafe { libc::poll(&mut descriptor, 1, millis) };
            if result > 0 {
                return Ok(());
            }
            if result == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "daemon exchange deadline elapsed",
                ));
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

fn nonblocking_socket() -> io::Result<OwnedFd> {
    // SAFETY: socket returns an owned descriptor on success. OwnedFd closes it on error.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: raw is a fresh descriptor returned by socket above.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    // SAFETY: fcntl reads the status flags of this valid, owned descriptor.
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl changes only this valid, owned descriptor's file status flags.
    if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd)
}

impl Read for DeadlineStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            self.wait(libc::POLLIN)?;
            match self.stream.read(buffer) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                result => return result,
            }
        }
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        loop {
            self.wait(libc::POLLOUT)?;
            match self.stream.write(buffer) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.remaining().map(|_| ())
    }
}

fn socket_address(path: &Path) -> io::Result<libc::sockaddr_un> {
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: zero is the correct initial state for sockaddr_un, including sun_path.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.is_empty() || bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid daemon socket path",
        ));
    }
    address.sun_family = libc::AF_UNIX as _;
    for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
        *target = *byte as libc::c_char;
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::os::unix::net::UnixListener;
    use std::thread;

    #[test]
    fn saturated_writer_observes_whole_exchange_deadline() {
        let (stream, _peer) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut transport = DeadlineStream {
            stream,
            deadline: Instant::now() + Duration::from_millis(50),
        };
        let started = Instant::now();
        let error = transport.write_all(&vec![0; 8 * 1024 * 1024]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn slow_drip_cannot_extend_the_response_deadline() {
        let (stream, mut peer) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut transport = DeadlineStream {
            stream,
            deadline: Instant::now() + Duration::from_millis(55),
        };
        let writer = thread::spawn(move || {
            for _ in 0..3 {
                thread::sleep(Duration::from_millis(20));
                let _ = peer.write_all(&[1]);
            }
        });
        let started = Instant::now();
        let error = transport.read_exact(&mut [0; 3]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
        writer.join().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn saturated_accept_backlog_observes_connect_deadline() {
        let temp = tempfile::TempDir::new().unwrap();
        if !crate::process::sandbox_probe::unix_socket_bindable(temp.path()) {
            return;
        }
        let path = temp.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).unwrap();
        // The exact number of pending sockets admitted with backlog zero is
        // kernel-dependent. Keep them open until one connect reaches its bound.
        assert_eq!(
            // SAFETY: listener owns a valid socket descriptor for the duration of this call.
            unsafe { libc::listen(listener.as_raw_fd(), 0) },
            0
        );
        let mut pending = Vec::new();
        for _ in 0..32 {
            let started = Instant::now();
            match DeadlineStream::connect(&path, Duration::from_millis(50)) {
                Ok(stream) => pending.push(stream),
                Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                    assert!(!pending.is_empty());
                    assert!(started.elapsed() < Duration::from_secs(1));
                    return;
                }
                Err(error) => panic!("unexpected connect failure: {error}"),
            }
        }
        panic!("accept backlog did not saturate within 32 pending connections");
    }
}
