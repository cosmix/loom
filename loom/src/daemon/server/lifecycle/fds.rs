use std::io;
use std::os::fd::RawFd;

pub(super) fn redirect_fd(source: RawFd, target: RawFd) -> io::Result<()> {
    loop {
        // SAFETY: dup2 validates the raw descriptors and replaces target atomically.
        if unsafe { libc::dup2(source, target) } >= 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
