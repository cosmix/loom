//! Reclaim a session's tmux server after its project owner has stayed absent.

use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::orchestrator::terminal::tmux::{socket_path_for, teardown_socket};
use crate::process::{verify_process_identity, IdentityStatus, ProcessIdentity};

pub const OWNER_GRACE: Duration = Duration::from_secs(15 * 60);
pub const OWNER_POLL: Duration = Duration::from_secs(15);

/// What the watch asks of the world; a test supplies a fake.
pub trait OwnerProbe {
    /// A server holds the project: the lock record names a live process.
    fn owner_alive(&mut self) -> bool;
    /// The session's tmux server still exists, even if Claude has exited.
    fn session_alive(&mut self) -> bool;
    fn teardown(&mut self);
    fn sleep(&mut self, duration: Duration);
    fn now(&mut self) -> Instant;
}

/// Returns when the session is gone, or tears it down after `grace` without an owner.
pub fn run(probe: &mut dyn OwnerProbe, grace: Duration, poll: Duration) {
    let mut last_owner = probe.now();
    loop {
        if !probe.session_alive() {
            return;
        }
        if probe.owner_alive() {
            last_owner = probe.now();
        } else if probe.now().saturating_duration_since(last_owner) >= grace {
            probe.teardown();
            return;
        }
        probe.sleep(poll);
    }
}

/// Read the server's JSON pid without taking its singleton lock or following a symlink.
pub fn read_holder_pid(path: &Path) -> Option<u32> {
    read_holder(path).map(|holder| holder.pid)
}

/// The identity in the server's lock record, read as [`read_holder_pid`] says.
fn read_holder(path: &Path) -> Option<ProcessIdentity> {
    #[derive(Deserialize)]
    struct Holder {
        pid: u32,
        #[serde(default)]
        start_time: Option<u64>,
    }

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(4096).read_to_end(&mut bytes).ok()?;
    let holder: Holder = serde_json::from_slice(&bytes).ok()?;
    (holder.pid != 0).then_some(ProcessIdentity {
        pid: holder.pid,
        start_time: holder.start_time,
    })
}

/// Production owner and tmux probes for a single planning session.
pub struct SessionOwnerProbe {
    lock_path: PathBuf,
    socket: String,
}

impl SessionOwnerProbe {
    pub fn new(lock_path: &Path, socket: &str) -> Self {
        Self {
            lock_path: lock_path.to_owned(),
            socket: socket.to_owned(),
        }
    }

    /// Watch with the production grace and polling intervals.
    pub fn watch(&mut self) {
        run(self, OWNER_GRACE, OWNER_POLL);
    }
}

impl OwnerProbe for SessionOwnerProbe {
    fn owner_alive(&mut self) -> bool {
        // The record outlives a killed server, and its pid can come back as another process.
        // Without a start time on both sides only the pid's existence can be checked.
        read_holder(&self.lock_path)
            .is_some_and(|holder| verify_process_identity(holder) != IdentityStatus::Dead)
    }

    fn session_alive(&mut self) -> bool {
        // A dead Claude identity can leave a remain-on-exit tmux server behind.
        // Keep watching that socket until the same owner-absence grace expires.
        std::fs::symlink_metadata(socket_path_for(&self.socket)).is_ok()
    }

    fn teardown(&mut self) {
        if let Err(error) = teardown_socket(&self.socket) {
            tracing::warn!(socket = %self.socket, %error, "owner watch teardown failed");
        }
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }

    fn now(&mut self) -> Instant {
        Instant::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_secs(60);
    const POLL: Duration = Duration::from_secs(15);

    struct FakeProbe {
        start: Instant,
        elapsed: Duration,
        owners: &'static [(u64, u64)],
        session_ends: Option<u64>,
        teardowns: Vec<Duration>,
    }

    impl FakeProbe {
        fn new(owners: &'static [(u64, u64)], session_ends: Option<u64>) -> Self {
            Self {
                start: Instant::now(),
                elapsed: Duration::ZERO,
                owners,
                session_ends,
                teardowns: Vec::new(),
            }
        }
    }

    impl OwnerProbe for FakeProbe {
        fn owner_alive(&mut self) -> bool {
            let second = self.elapsed.as_secs();
            self.owners
                .iter()
                .any(|&(from, to)| (from..to).contains(&second))
        }

        fn session_alive(&mut self) -> bool {
            self.session_ends
                .is_none_or(|end| self.elapsed.as_secs() < end)
        }

        fn teardown(&mut self) {
            self.teardowns.push(self.elapsed);
            assert_eq!(self.teardowns.len(), 1, "teardown must not be repeated");
        }

        fn sleep(&mut self, duration: Duration) {
            assert!(!duration.is_zero(), "the watch must advance its clock");
            self.elapsed += duration;
            assert!(
                self.elapsed < Duration::from_secs(300),
                "watch never returned"
            );
        }

        fn now(&mut self) -> Instant {
            self.start + self.elapsed
        }
    }

    #[test]
    fn grace_expiry_tears_down() {
        let mut probe = FakeProbe::new(&[], None);

        run(&mut probe, GRACE, POLL);

        assert_eq!(probe.teardowns, [Duration::from_secs(60)]);
    }

    #[test]
    fn owner_return_within_grace_keeps_session() {
        let mut probe = FakeProbe::new(&[(45, 60)], Some(90));

        run(&mut probe, GRACE, POLL);

        assert!(probe.teardowns.is_empty());
        assert_eq!(probe.elapsed, Duration::from_secs(90));
    }

    #[test]
    fn every_owner_sighting_resets_grace() {
        let mut probe = FakeProbe::new(&[(30, 75)], None);

        run(&mut probe, GRACE, POLL);

        assert_eq!(probe.teardowns, [Duration::from_secs(120)]);
    }

    #[test]
    fn gone_session_returns_immediately() {
        let mut probe = FakeProbe::new(&[], Some(0));

        run(&mut probe, GRACE, POLL);

        assert!(probe.teardowns.is_empty());
        assert_eq!(probe.elapsed, Duration::ZERO);
    }

    #[test]
    fn teardown_returns_even_if_socket_remains() {
        let mut probe = FakeProbe::new(&[], None);

        run(&mut probe, Duration::ZERO, POLL);

        assert_eq!(probe.teardowns, [Duration::ZERO]);
        assert_eq!(probe.elapsed, Duration::ZERO);
    }

    #[test]
    fn holder_read_accepts_the_json_record() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("server.lock");
        std::fs::write(
            &path,
            br#"{"pid":42,"url":"http://localhost","token":"secret"}"#,
        )
        .unwrap();

        assert_eq!(read_holder_pid(&path), Some(42));
    }

    #[test]
    fn holder_read_rejects_missing_invalid_and_zero_pids() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("server.lock");
        assert_eq!(read_holder_pid(&path), None);
        for content in [
            "invalid",
            "{}",
            r#"{"pid":0}"#,
            r#"{"pid":-1}"#,
            r#"{"pid":4294967296}"#,
            r#"{"pid":"42"}"#,
        ] {
            std::fs::write(&path, content).unwrap();
            assert_eq!(read_holder_pid(&path), None, "accepted {content}");
        }
    }

    #[test]
    fn holder_read_rejects_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("record");
        let link = temp.path().join("server.lock");
        std::fs::write(&target, br#"{"pid":42}"#).unwrap();
        std::os::unix::fs::symlink(target, &link).unwrap();

        assert_eq!(read_holder_pid(&link), None);
    }

    #[test]
    fn holder_read_rejects_oversized_records() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("server.lock");
        let mut content = br#"{"pid":42}"#.to_vec();
        content.resize(4097, b' ');
        std::fs::write(&path, content).unwrap();

        assert_eq!(read_holder_pid(&path), None);
    }

    /// What the production probe answers for a record naming this live process.
    fn owner_alive_with_record(record: serde_json::Value) -> bool {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("server.lock");
        std::fs::write(&path, record.to_string()).unwrap();
        SessionOwnerProbe::new(&path, "unused-socket").owner_alive()
    }

    #[test]
    fn a_reused_pid_reads_as_no_owner() {
        let pid = std::process::id();
        let started = crate::process::process_start_time(pid).expect("this process's start time");

        let reused = serde_json::json!({"pid": pid, "start_time": started + 1});
        assert!(!owner_alive_with_record(reused));

        let same = serde_json::json!({"pid": pid, "start_time": started});
        assert!(owner_alive_with_record(same));
    }

    #[test]
    fn a_record_without_a_start_time_falls_back_to_the_pid() {
        let live = serde_json::json!({"pid": std::process::id()});
        assert!(owner_alive_with_record(live));

        let mut child = std::process::Command::new("true").spawn().unwrap();
        let dead = child.id();
        child.wait().unwrap();
        assert!(!owner_alive_with_record(serde_json::json!({"pid": dead})));
    }
}
