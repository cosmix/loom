//! Recovery of a killed status TUI in a later process. Crossterm's saved raw
//! mode lives in the original process, so it cannot restore this state.

use std::fs::Metadata;
use std::io::{self, IsTerminal};
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use nix::sys::termios::{
    tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags, SetArg,
};
use nix::unistd::{dup, ttyname};

struct TerminalIdentity {
    path: PathBuf,
    device: u64,
    inode: u64,
    rdevice: u64,
}

impl TerminalIdentity {
    fn for_fd(fd: &OwnedFd) -> io::Result<Self> {
        let path = ttyname(fd).map_err(io::Error::from)?;
        let metadata = std::fs::metadata(&path)?;
        Ok(Self::from_metadata(path, &metadata))
    }

    fn from_metadata(path: PathBuf, metadata: &Metadata) -> Self {
        Self {
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
            rdevice: metadata.rdev(),
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.path == other.path
            && self.device == other.device
            && self.inode == other.inode
            && self.rdevice == other.rdevice
    }
}

/// Crossterm chooses stdin when it is a TTY, otherwise `/dev/tty`.
fn terminal_fd() -> io::Result<OwnedFd> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return dup(&stdin).map_err(io::Error::from);
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map(OwnedFd::from)
}

struct Marker {
    pid: u32,
    terminal: TerminalIdentity,
}

impl Marker {
    fn parse(contents: &str) -> Option<Self> {
        let mut lines = contents.lines();
        if lines.next()? != "v2" {
            return None;
        }
        let pid = lines.next()?.parse().ok()?;
        let path = PathBuf::from(lines.next()?);
        let device = lines.next()?.parse().ok()?;
        let inode = lines.next()?.parse().ok()?;
        let rdevice = lines.next()?.parse().ok()?;
        if lines.next().is_some() {
            return None;
        }
        Some(Self {
            pid,
            terminal: TerminalIdentity {
                path,
                device,
                inode,
                rdevice,
            },
        })
    }
}

pub(super) fn marker_record() -> Option<String> {
    let fd = terminal_fd().ok()?;
    let terminal = TerminalIdentity::for_fd(&fd).ok()?;
    Some(format!(
        "v2\n{}\n{}\n{}\n{}\n{}\n",
        std::process::id(),
        terminal.path.display(),
        terminal.device,
        terminal.inode,
        terminal.rdevice
    ))
}

pub(super) fn marker_pid(contents: &str) -> Option<u32> {
    Marker::parse(contents).map(|marker| marker.pid)
}

/// Restore a usable canonical/echo mode on the same PTY. The prior exact
/// termios attributes were not saved, so this deliberately restores only the
/// flags that crossterm raw mode disables and normal command input needs.
pub(super) fn restore_stale_terminal(contents: &str) -> io::Result<()> {
    let marker = Marker::parse(contents)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid TUI marker"))?;
    if !io::stdout().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "stdout is not a terminal",
        ));
    }
    let fd = terminal_fd()?;
    let current = TerminalIdentity::for_fd(&fd)?;
    let stdout_fd = dup(io::stdout()).map_err(io::Error::from)?;
    let output = TerminalIdentity::for_fd(&stdout_fd)?;
    if !marker.terminal.matches(&current) || !marker.terminal.matches(&output) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "TUI marker belongs to another terminal",
        ));
    }

    let mut attributes = tcgetattr(&fd).map_err(io::Error::from)?;
    attributes
        .local_flags
        .insert(LocalFlags::ECHO | LocalFlags::ICANON | LocalFlags::ISIG | LocalFlags::IEXTEN);
    attributes
        .input_flags
        .insert(InputFlags::BRKINT | InputFlags::ICRNL);
    attributes.output_flags.insert(OutputFlags::OPOST);
    attributes.control_flags.insert(ControlFlags::CREAD);
    tcsetattr(&fd, SetArg::TCSANOW, &attributes).map_err(io::Error::from)
}

#[cfg(test)]
mod tests {
    use super::{Marker, TerminalIdentity};
    use std::path::PathBuf;

    #[test]
    fn marker_requires_terminal_identity() {
        assert!(Marker::parse("123\n").is_none());
        assert!(Marker::parse("v2\n123\n/dev/pts/1\n1\n2\n3\nextra\n").is_none());
        let marker = Marker::parse("v2\n123\n/dev/pts/1\n1\n2\n3\n").unwrap();
        assert_eq!(marker.pid, 123);
        assert!(marker.terminal.matches(&TerminalIdentity {
            path: PathBuf::from("/dev/pts/1"),
            device: 1,
            inode: 2,
            rdevice: 3,
        }));
    }
}
