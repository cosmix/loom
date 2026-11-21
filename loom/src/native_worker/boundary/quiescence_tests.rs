//! The boundary kill signals only the boundary's own processes: on Linux the
//! members of the namespace it names, on macOS the session, group and marker
//! matches the census finds.
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
use super::kill_namespace;
#[cfg(target_os = "linux")]
use super::Quiescence;
#[cfg(target_os = "linux")]
use crate::test_stand_in::{require_signals, StandIn};

/// A host process named with another namespace is never signalled: a pid the
/// boundary's first process left behind and the host reused stays alive.
#[cfg(target_os = "linux")]
#[test]
fn killing_a_namespace_spares_a_host_process_outside_it() {
    require_signals!();
    let mut sleep = StandIn::direct(Command::new("sleep").arg("30"));
    let pid = i32::try_from(sleep.id()).unwrap();
    // No pid namespace has inode 1, so it has no members either.
    let quiescence = kill_namespace(pid, 1);
    std::thread::sleep(Duration::from_millis(100));
    let alive = sleep.child().try_wait().unwrap().is_none();
    drop(sleep);
    assert_eq!(quiescence, Quiescence::Released);
    assert!(alive, "a process outside the namespace was killed");
}

#[cfg(target_os = "macos")]
mod macos {
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    use super::super::{kill_mac, verify_quiescent};
    use crate::native_worker::boundary::quiescence_scan::{
        survivors, BoundaryMark, ProcEntry, BOUNDARY_MARKER_VAR,
    };
    use crate::native_worker::boundary::Quiescence;
    use crate::native_worker::model::{BoundaryIdentity, MacIdentity};
    use crate::process::macos_procs::{census, EnvRead};
    use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
    use crate::process::{process_is_zombie, process_start_time, signal_group_id};

    /// Runs unconfined only, like every `native_macos_*` kernel test: a
    /// sandboxed host hides other processes' environments, so its census is
    /// never conclusive. Skips (or fails under
    /// `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`) when the host is confined, signals
    /// are refused, or the census cannot read the test's own session.
    #[test]
    fn native_macos_quiescence_finds_and_kills_a_fresh_session_and_its_children() {
        let name = "native_macos_quiescence_finds_and_kills_a_fresh_session_and_its_children";
        if skip_unless(
            confined_commands_runnable(),
            name,
            "needs an unconfined host",
        ) {
            return;
        }
        crate::test_stand_in::require_signals!();
        let invocation = format!("quiescence-test-{}", std::process::id());
        let mut child = SessionChild::spawn(&invocation);
        if census_refused(
            name,
            &format!("{BOUNDARY_MARKER_VAR}={invocation}"),
            &child.0,
        ) {
            return;
        }
        let mac = identity_of(&child.0, invocation);
        let members = members_within(&mac.mark(), Duration::from_secs(5));
        assert!(members.len() >= 2, "the scan found {members:?}");
        assert!(members.contains(&mac.session), "the leader is not a member");
        let identity = BoundaryIdentity {
            supervisor_pid: std::process::id() as i32,
            supervisor_start: "0".into(),
            boundary_pid: mac.session as i32,
            pid_namespace: 0,
            boot_id: mac.boot_id.clone(),
            mac: Some(mac.clone()),
        };
        let proof = verify_quiescent(&identity);
        assert!(matches!(proof, Quiescence::Unverified(_)), "{proof:?}");
        let killed = Instant::now();
        assert_eq!(kill_mac(&mac), Quiescence::Released);
        assert!(killed.elapsed() < Duration::from_secs(30));
        for pid in members {
            let runs = process_start_time(pid).is_some() && !process_is_zombie(pid);
            assert!(!runs, "member {pid} still runs");
        }
        child.reap();
    }

    /// Whether the census cannot read the environment of `leader`, a
    /// process of this test; a confined host refuses it (`EINVAL`).
    fn census_refused(name: &str, marker: &str, leader: &Child) -> bool {
        let refused = census(marker)
            .records
            .iter()
            .any(|record| record.pid == leader.id() && matches!(record.env, EnvRead::Denied(_)));
        skip_unless(!refused, name, "the process census is refused on this host")
    }

    fn identity_of(leader: &Child, invocation: String) -> MacIdentity {
        let session = leader.id();
        MacIdentity {
            invocation,
            session,
            leader_start: process_start_time(session)
                .expect("leader start time")
                .to_string(),
            boot_id: crate::process::boot_id::os_boot_id().expect("boot id"),
            scratch_root: std::env::temp_dir().join("native").join("unused"),
        }
    }

    /// The scan's survivors once it finds at least two, or after `bound`.
    fn members_within(mark: &BoundaryMark, bound: Duration) -> Vec<u32> {
        let started = Instant::now();
        loop {
            let entries: Vec<ProcEntry> = census(&mark.marker)
                .records
                .into_iter()
                .filter(|r| !r.zombie && r.sid.is_some())
                .map(|r| ProcEntry {
                    pid: r.pid,
                    ppid: r.ppid,
                    pgid: r.pgid,
                    sid: r.sid.unwrap_or(0),
                    marker: match r.env {
                        EnvRead::Read(marker) => marker,
                        _ => None,
                    },
                })
                .collect();
            let found = survivors(&entries, mark);
            if found.len() >= 2 || started.elapsed() > bound {
                return found;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// `sh -c 'sleep 30 & sleep 30'` in a new session with the marker. Dropping
    /// it kills its group while the leader is unreaped, so the number names no
    /// other group.
    struct SessionChild(Child);

    impl SessionChild {
        fn spawn(invocation: &str) -> Self {
            let mut command = Command::new("/bin/sh");
            command
                .args(["-c", "sleep 30 & sleep 30"])
                .env(BOUNDARY_MARKER_VAR, invocation);
            // SAFETY: `setsid` is async-signal-safe and touches no parent memory.
            unsafe {
                command.pre_exec(|| match libc::setsid() {
                    -1 => Err(std::io::Error::last_os_error()),
                    _ => Ok(()),
                })
            };
            Self(command.spawn().expect("spawn the session"))
        }

        fn reap(&mut self) {
            let _ = crate::process::wait_with_deadline(&mut self.0, Duration::from_secs(5));
        }
    }

    impl Drop for SessionChild {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                if let Ok(group) = i32::try_from(self.0.id()) {
                    signal_group_id(group, nix::sys::signal::Signal::SIGKILL);
                }
            }
            let _ = self.0.kill();
            self.reap();
        }
    }
}
