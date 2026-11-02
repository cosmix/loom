//! Deadline-bound writes for daemon responses and subscriber frames.

use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use super::SUBSCRIBER_WRITE_TIMEOUT;

pub(in crate::daemon::server) fn write_bounded_frame(
    stream: &mut UnixStream,
    frame: &[u8],
) -> io::Result<()> {
    write_bounded_frame_with_timeout(stream, frame, SUBSCRIBER_WRITE_TIMEOUT)
}

pub(in crate::daemon::server) fn write_bounded_frame_with_timeout(
    stream: &mut UnixStream,
    frame: &[u8],
    timeout: Duration,
) -> io::Result<()> {
    let result = write_frame_with_timeout(stream, frame, timeout);
    if result.is_err() {
        // A partial frame cannot be resumed by another response writer.
        // Closing the write half makes every retained channel snapshot and
        // subsequent control reply fail on this same connection.
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }
    result
}

fn write_frame_with_timeout(
    stream: &mut UnixStream,
    frame: &[u8],
    timeout: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    let mut remaining_frame = frame;
    while !remaining_frame.is_empty() {
        write_time_remaining(deadline)?;
        // SO_SNDTIMEO bounds a blocking send only while it makes no progress.
        // MSG_DONTWAIT keeps each call bounded without changing the shared
        // socket mode seen by the connection's reader.
        #[cfg(target_os = "linux")]
        let flags = libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL;
        #[cfg(not(target_os = "linux"))]
        let flags = libc::MSG_DONTWAIT;
        // SAFETY: the descriptor is a live Unix socket and remaining_frame is
        // a valid byte slice for the duration of this call.
        let written = unsafe {
            libc::send(
                stream.as_raw_fd(),
                remaining_frame.as_ptr().cast(),
                remaining_frame.len(),
                flags,
            )
        };
        let result = if written < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(written as usize)
        };
        match result {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => remaining_frame = &remaining_frame[written..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait_for_write(stream.as_raw_fd(), deadline)?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn write_time_remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "response write deadline elapsed"))
}

fn wait_for_write(fd: RawFd, deadline: Instant) -> io::Result<()> {
    loop {
        let millis = write_time_remaining(deadline)?
            .as_millis()
            .clamp(1, i32::MAX as u128) as libc::c_int;
        let mut descriptor = libc::pollfd {
            fd,
            events: libc::POLLOUT,
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
                "response write deadline elapsed",
            ));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
