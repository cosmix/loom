//! Shared lifecycle fixtures: leader exits with an inheritor, reply or no reply.
use super::{is_process_alive, OwnedProcess};
use crate::test_stand_in::Orphan;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The `sleep 30` a fixture's shell leaves holding its stdout, adopted by a
/// watcher the moment the shell writes its pid, while it is still the process
/// the shell started. A test that fails before loom retires it kills it on drop.
/// A test using it starts with `require_orphan_signals!()`.
pub(crate) struct InheritorGuard {
    pub(crate) pid_file: PathBuf,
    inheritor: Arc<Mutex<Option<Orphan>>>,
    stop: Arc<AtomicBool>,
    watcher: Option<JoinHandle<()>>,
}

/// Poll `pid_file` until it holds a pid, adopting that process into `slot`,
/// or until `stop` is set.
fn adopt_when_written(pid_file: &Path, slot: &Mutex<Option<Orphan>>, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        let written = std::fs::read_to_string(pid_file).ok();
        if let Some(pid) = written.and_then(|text| text.trim().parse().ok()) {
            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(Orphan::adopt(pid));
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

impl InheritorGuard {
    pub(crate) fn new(directory: &Path) -> Self {
        let pid_file = directory.join("child.pid");
        let inheritor = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = {
            let (file, slot, stop) = (pid_file.clone(), inheritor.clone(), stop.clone());
            std::thread::spawn(move || adopt_when_written(&file, &slot, &stop))
        };
        Self {
            pid_file,
            inheritor,
            stop,
            watcher: Some(watcher),
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
        self.stop.store(true, Ordering::Relaxed);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
        let adopted = self
            .inheritor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        drop(adopted);
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
    crate::test_stand_in::require_orphan_signals!();
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

#[test]
fn wait_leaves_the_leader_unreaped_until_retire() {
    use super::{process_is_zombie, process_start_time};
    use std::os::unix::process::CommandExt;

    let child = std::process::Command::new("sh")
        .args(["-c", "exit 3"])
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = child.id();
    let mut owner = OwnedProcess::new(child);

    let status = owner
        .wait(Duration::from_secs(10))
        .unwrap()
        .expect("the child exits within the timeout");
    assert_eq!(status.code(), Some(3));
    assert!(process_is_zombie(pid), "wait reaped the leader");
    assert!(process_start_time(pid).is_some(), "the pid was released");

    owner.retire();
    assert!(process_start_time(pid).is_none(), "retire did not reap");
}
