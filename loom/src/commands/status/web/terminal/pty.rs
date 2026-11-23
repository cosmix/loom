use std::io;
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::fcntl::open;
#[cfg(not(target_os = "linux"))]
use nix::fcntl::FdFlag;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
#[cfg(not(target_os = "linux"))]
use nix::pty::openpty;
use nix::pty::Winsize;
#[cfg(target_os = "linux")]
use nix::pty::{grantpt, posix_openpt, ptsname_r, unlockpt, PtyMaster};
#[cfg(target_os = "linux")]
use nix::sys::stat::Mode;

use super::protocol::WindowSize;

/// A child process whose controlling terminal is a PTY we own.
pub(super) struct PtyChild {
    master: OwnedFd,
    child: Child,
}

impl PtyChild {
    /// Open a sized PTY and give its slave to the child as its controlling terminal.
    pub(super) fn spawn(command: &mut Command, size: WindowSize) -> std::io::Result<Self> {
        let (master, slave) = open_pty(size)?;
        let flags = fcntl(&master, FcntlArg::F_GETFL).map_err(io::Error::from)?;
        let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
        fcntl(&master, FcntlArg::F_SETFL(flags)).map_err(io::Error::from)?;

        // The slave clones are CLOEXEC (`try_clone` uses `F_DUPFD_CLOEXEC`),
        // and the child's stdin/stdout/stderr are `dup2`'d from them, which
        // clears CLOEXEC on those destination fds as intended.
        command.stdin(Stdio::from(slave.try_clone()?));
        command.stdout(Stdio::from(slave.try_clone()?));
        command.stderr(Stdio::from(slave.try_clone()?));
        // Safety: only async-signal-safe session/ioctl operations run after fork.
        unsafe {
            command.pre_exec(|| {
                nix::unistd::setsid().map_err(io::Error::from)?;
                libc::ioctl(0, libc::TIOCSCTTY as _, 0);
                Ok(())
            });
        }
        let child = command.spawn()?;
        drop(slave);
        // `command` outlives this call in every caller (it drives the bridge
        // session for as long as the WebSocket is open), and `Command` keeps
        // the stdio fds it was given alive until it is dropped or reassigned.
        // Left alone, the three slave clones above would stay open for that
        // whole lifetime, so the master would never see the child's hangup
        // when it exits. Overwriting them here drops those clones now that
        // the child has its own copies from `spawn`.
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        Ok(Self { master, child })
    }

    pub(super) fn master(&self) -> BorrowedFd<'_> {
        self.master.as_fd()
    }

    /// Change the PTY window size, causing the child session to receive SIGWINCH.
    pub(super) fn resize(&self, size: WindowSize) -> io::Result<()> {
        set_winsize(self.master.as_fd(), size)
    }

    /// Read once without blocking; `None` means that no bytes are ready.
    pub(super) fn read(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        match nix::unistd::read(&self.master, buf) {
            Ok(count) => Ok(Some(count)),
            Err(Errno::EAGAIN) => Ok(None),
            Err(Errno::EIO) => Ok(Some(0)),
            Err(error) => Err(io::Error::from(error)),
        }
    }

    /// Write once without blocking; zero means that the PTY is not writable yet.
    pub(super) fn write_some(&self, bytes: &[u8]) -> io::Result<usize> {
        match nix::unistd::write(&self.master, bytes) {
            Ok(count) => Ok(count),
            Err(Errno::EAGAIN) => Ok(0),
            Err(error) => Err(io::Error::from(error)),
        }
    }

    #[cfg(test)]
    pub(super) fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Hang up the PTY, wait up to two seconds, then kill and reap the child.
    /// Returns how the child ended: killed by `SIGHUP` when the hangup reached
    /// it, by `SIGKILL` when the deadline passed first, `None` when it could
    /// not be reaped.
    pub(super) fn shutdown(self) -> Option<ExitStatus> {
        let Self { master, mut child } = self;
        drop(master);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if Instant::now() < deadline => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    thread::sleep(Duration::from_millis(50).min(remaining));
                }
                Ok(None) | Err(_) => break,
            }
        }
        let _ = child.kill();
        child.wait().ok()
    }
}

fn set_winsize(master: BorrowedFd<'_>, size: WindowSize) -> io::Result<()> {
    let winsize = winsize(size);
    // Safety: the master fd is live and the pointer references a complete winsize.
    let result = unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &winsize as *const _) };
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Open a sized PTY whose master and slave are close-on-exec from the moment
/// they exist. A child that is `fork`ed on another thread while a master
/// still lacks `O_CLOEXEC` keeps its copy across its own exec, and the copy
/// keeps the slave from ever seeing a hangup: `shutdown`'s `drop(master)`
/// stops working and the close pays its two-second kill fallback until that
/// unrelated process exits. `openpty(3)` has no flags argument, so setting
/// the flag after it returns leaves exactly that window. The slave is the
/// peer of this master, taken from the kernel, with its `/dev/pts` path as
/// the fallback.
#[cfg(target_os = "linux")]
pub(super) fn open_pty(size: WindowSize) -> io::Result<(OwnedFd, OwnedFd)> {
    let flags = OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC;
    let master = posix_openpt(flags).map_err(io::Error::from)?;
    grantpt(&master).map_err(io::Error::from)?;
    unlockpt(&master).map_err(io::Error::from)?;
    let slave = open_slave(&master, flags)?;
    set_winsize(master.as_fd(), size)?;
    Ok((master.into(), slave))
}

/// The slave of exactly this master. `TIOCGPTPEER` (Linux 4.13) asks the
/// kernel for it, so a container or sandbox whose `/dev/pts` is a different
/// devpts instance than the master's cannot misdirect the open. A seccomp
/// filter or an older kernel can refuse the ioctl; any failure falls back to
/// opening the path from `ptsname_r`, whose own error is the one returned.
#[cfg(target_os = "linux")]
fn open_slave(master: &PtyMaster, flags: OFlag) -> io::Result<OwnedFd> {
    // Safety: the master fd is live and the flags argument is a plain int.
    let peer = unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCGPTPEER, flags.bits()) };
    if peer >= 0 {
        // Safety: the ioctl returned a new descriptor that nothing else owns.
        return Ok(unsafe { OwnedFd::from_raw_fd(peer) });
    }
    let slave_path = ptsname_r(master).map_err(io::Error::from)?;
    open(slave_path.as_str(), flags, Mode::empty()).map_err(io::Error::from)
}

/// `openpty(3)` and a `FD_CLOEXEC` set on the master after it returns. The
/// master is open without the flag for the instant between the two (see the
/// Linux variant); `ptsname_r`, which the Linux variant needs, is a Linux
/// extension, and the portable `ptsname` is not thread-safe.
#[cfg(not(target_os = "linux"))]
pub(super) fn open_pty(size: WindowSize) -> io::Result<(OwnedFd, OwnedFd)> {
    let winsize = winsize(size);
    let pair = openpty(Some(&winsize), None).map_err(io::Error::from)?;
    fcntl(&pair.master, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).map_err(io::Error::from)?;
    Ok((pair.master, pair.slave))
}

fn winsize(size: WindowSize) -> Winsize {
    Winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}
