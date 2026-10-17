//! Shared lifecycle fixtures: leader exits with an inheritor, reply or no reply.
use super::{is_process_alive, OwnedProcess};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) struct InheritorGuard {
    pub(crate) pid_file: PathBuf,
}
impl InheritorGuard {
    pub(crate) fn new(directory: &Path) -> Self {
        Self {
            pid_file: directory.join("child.pid"),
        }
    }
    pub(crate) fn shell(&self) -> String {
        format!(
            "sleep 30 & echo $! > {};",
            shell_escape::escape(self.pid_file.to_string_lossy())
        )
    }
    pub(crate) fn assert_retired(&self) {
        let pid: u32 = std::fs::read_to_string(&self.pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while is_process_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!is_process_alive(pid), "owned descendant {pid} survived");
    }
}
impl Drop for InheritorGuard {
    fn drop(&mut self) {
        if let Some(pid) = std::fs::read_to_string(&self.pid_file)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
        {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
            let _ = nix::sys::wait::waitpid(
                nix::unistd::Pid::from_raw(pid),
                Some(nix::sys::wait::WaitPidFlag::WNOHANG),
            );
        }
    }
}

#[test]
fn ownership_waits_for_a_child_that_exits_after_wait_begins() {
    use std::os::unix::process::CommandExt;

    let child = std::process::Command::new("sh")
        .args(["-c", "sleep 0.1; exit 0"])
        .process_group(0)
        .spawn()
        .unwrap();
    let mut owner = OwnedProcess::new(child);

    let status = owner
        .wait(Duration::from_secs(1))
        .expect("wait must not depend on a SIGCHLD handler")
        .expect("completed child must return its exit status");

    assert!(status.success());
}

#[test]
fn ownership_drop_retires_group_after_leader_exit() {
    use std::os::unix::process::CommandExt;
    let temp = tempfile::tempdir().unwrap();
    let guard = InheritorGuard::new(temp.path());
    let child = std::process::Command::new("sh")
        .args(["-c", &guard.shell()])
        .process_group(0)
        .spawn()
        .unwrap();
    let mut owner = OwnedProcess::new(child);
    assert!(owner.wait(Duration::from_secs(5)).unwrap().is_some());
    drop(owner);
    guard.assert_retired();
}
