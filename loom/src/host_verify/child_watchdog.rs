//! A verify-host child stops its own group when its current job is withdrawn.
//! The new daemon only changes durable state; it never signals a recycled PID.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use nix::sys::signal::{killpg, Signal};
use nix::unistd::{getpgrp, getpid};

use super::{store, JobState};

pub(super) struct Watchdog(Arc<AtomicBool>);

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub(super) fn start(work_dir: &Path, stage_id: &str, job_id: &str) -> Option<Watchdog> {
    // Direct in-process callers used by tests do not own their process group.
    // The production spawn sets both the work-root marker and pgid == pid.
    let launched_root = std::env::var_os("LOOM_WORK_DIR")?;
    if Path::new(&launched_root).canonicalize().ok()? != work_dir.canonicalize().ok()? {
        return None;
    }
    let group = getpgrp();
    if group != getpid() {
        return None;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let (work_dir, stage_id, job_id) = (
        work_dir.to_path_buf(),
        stage_id.to_owned(),
        job_id.to_owned(),
    );
    let running = stop.clone();
    let _ = std::thread::spawn(move || {
        while !running.load(Ordering::Acquire) {
            let still_current = store::load_current(&work_dir, &stage_id).is_ok_and(|record| {
                record
                    .is_some_and(|record| record.id == job_id && record.state == JobState::Running)
            });
            if !still_current {
                if !running.load(Ordering::Acquire) {
                    let _ = killpg(group, Signal::SIGKILL);
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    Some(Watchdog(stop))
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Command, Stdio};
    use std::time::Instant;

    use super::*;
    use crate::host_verify::store::save_current;
    use crate::host_verify::tests_runner::{state_root, STAGE};
    use crate::host_verify::tests_runner_recovery::running_record;

    const CHILD_ENV: &str = "LOOM_HOST_WATCHDOG_TEST_CHILD";

    #[test]
    fn an_interrupted_owned_child_stops_its_own_group() {
        if std::env::var_os(CHILD_ENV).is_some() {
            run_child_until_interrupted();
        }
        let (_dir, work) = state_root();
        let mut record = running_record(STAGE, None);
        save_current(&work, &record).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "host_verify::child_watchdog::tests::an_interrupted_owned_child_stops_its_own_group"])
            .env(CHILD_ENV, "1")
            .env("LOOM_WORK_DIR", &work)
            .env("LOOM_HOST_WATCHDOG_JOB", &record.id)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !work.join("watchdog-ready").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !work.join("watchdog-ready").exists() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("watchdog child did not start");
        }
        record.state = JobState::Interrupted;
        save_current(&work, &record).unwrap();
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                assert_eq!(status.signal(), Some(libc::SIGKILL));
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("watchdog did not stop the child within five seconds");
    }

    fn run_child_until_interrupted() {
        let work = std::path::PathBuf::from(std::env::var_os("LOOM_WORK_DIR").unwrap());
        let job_id = std::env::var("LOOM_HOST_WATCHDOG_JOB").unwrap();
        let _watchdog = start(&work, STAGE, &job_id).expect("owned child installs watchdog");
        std::fs::write(work.join("watchdog-ready"), b"ready").unwrap();
        std::thread::sleep(Duration::from_secs(10));
        panic!("watchdog failed to stop the child");
    }
}
