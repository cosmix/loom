//! One absolute deadline for connecting, writing, and reading a daemon RPC.

use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) struct DeadlineStream {
    stream: UnixStream,
    deadline: Instant,
}

impl DeadlineStream {
    pub(super) fn connect(path: &Path, timeout: Duration) -> io::Result<Self> {
        let deadline = Instant::now() + timeout;
        let address = socket_address(path)?;
        let fd = nonblocking_socket()?;
        // SAFETY: address is fully initialized and the descriptor is an AF_UNIX socket.
        let connected = unsafe {
            libc::connect(
                fd.as_raw_fd(),
                (&raw const address).cast::<libc::sockaddr>(),
                std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
            )
        };
        let stream = UnixStream::from(fd);
        let transport = Self { stream, deadline };
        if connected == 0 {
            return Ok(transport);
        }
        let error = io::Error::last_os_error();
        if !matches!(error.raw_os_error(), Some(libc::EINPROGRESS | libc::EAGAIN)) {
            return Err(error);
        }
        transport.wait(libc::POLLOUT)?;
        transport.finish_connect()?;
        Ok(transport)
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
}
