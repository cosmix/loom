use anyhow::{ensure, Result};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) fn connect(path: &Path) -> Result<Connection> {
    // SAFETY: socket has no pointer arguments; ownership immediately moves
    // into UnixStream, including every error path after this point.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    ensure!(fd >= 0, "cannot create inheritance socket");
    // SAFETY: fd is a newly allocated, uniquely owned socket descriptor.
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    ensure!(
        // SAFETY: fd is live and F_SETFD takes an integer descriptor flag.
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == 0,
        "cannot protect socket descriptor"
    );
    stream.set_nonblocking(true)?;
    let address = socket_address(path)?;
    // SAFETY: address is an initialized sockaddr_un, including its terminator.
    let rc = unsafe {
        libc::connect(
            fd,
            (&raw const address).cast(),
            std::mem::size_of_val(&address) as _,
        )
    };
    if rc != 0 {
        ensure!(
            std::io::Error::last_os_error().raw_os_error() == Some(libc::EINPROGRESS),
            "inheritance connect refused"
        );
        wait_connected(&stream)?;
    }
    Ok(Connection {
        stream,
        deadline: Instant::now() + Duration::from_secs(2),
    })
}

fn socket_address(path: &Path) -> Result<libc::sockaddr_un> {
    // SAFETY: zero is valid for the sockaddr_un fields; the path is set below.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as _;
    let bytes = path.as_os_str().as_bytes();
    ensure!(
        bytes.len() < address.sun_path.len(),
        "inheritance socket path too long"
    );
    for (target, source) in address.sun_path.iter_mut().zip(bytes) {
        *target = *source as libc::c_char;
    }
    address.sun_len = std::mem::size_of_val(&address) as u8;
    Ok(address)
}

/// One total exchange deadline prevents a slow peer extending read_exact's
/// deadline by delivering one byte per socket timeout.
pub(super) struct Connection {
    stream: UnixStream,
    deadline: Instant,
}

impl Connection {
    pub(super) fn peer_pid(&self) -> Option<u32> {
        crate::daemon::server::peer_pid(&self.stream)
    }

    fn wait(&self, events: i16) -> io::Result<()> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut poll = libc::pollfd {
            fd: self.stream.as_raw_fd(),
            events,
            revents: 0,
        };
        let timeout = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        // SAFETY: poll points to one initialized writable pollfd.
        match unsafe { libc::poll(&raw mut poll, 1, timeout) } {
            0 => Err(io::ErrorKind::TimedOut.into()),
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

impl Read for Connection {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            self.wait(libc::POLLIN)?;
            match self.stream.read(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                result => return result,
            }
        }
    }
}

impl Write for Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.wait(libc::POLLOUT)?;
            match self.stream.write(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn wait_connected(stream: &UnixStream) -> Result<()> {
    let mut poll = libc::pollfd {
        fd: stream.as_raw_fd(),
        events: libc::POLLOUT,
        revents: 0,
    };
    ensure!(
        // SAFETY: poll points to one initialized writable pollfd.
        unsafe { libc::poll(&raw mut poll, 1, 2000) } > 0,
        "inheritance connect timed out"
    );
    ensure!(stream.take_error()?.is_none(), "inheritance connect failed");
    Ok(())
}
