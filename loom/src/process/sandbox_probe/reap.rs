//! Bounded cleanup for the stand-in processes tests spawn.
//!
//! A host job's sandbox profile may refuse signals to processes, and the refused
//! kill leaves the stand-in running. A cleanup that kills and then calls `wait()`
//! blocks for as long as the stand-in lives, which for a spin loop is forever and
//! for `sleep 600` ten minutes. These helpers wait for a bounded time and report
//! whether the child ended, so a refused kill costs the deadline and nothing more.

use std::process::Child;
use std::time::Duration;

/// How long a cleanup waits for a killed child to end.
pub const REAP_DEADLINE: Duration = Duration::from_secs(5);

/// Polls `try_wait` until the child ends or `deadline` passes. True when reaped.
pub fn reap_within(child: &mut Child, deadline: Duration) -> bool {
    matches!(
        crate::process::owned::wait_with_deadline(child, deadline),
        Ok(Some(_))
    )
}

/// Kills `child` and reaps it for at most [`REAP_DEADLINE`]. True when it ended.
pub fn kill_and_reap(child: &mut Child) -> bool {
    let _ = child.kill();
    reap_within(child, REAP_DEADLINE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn reap_within_reports_false_for_a_child_that_outlives_the_deadline() {
        let mut child = Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep");
        assert!(!reap_within(&mut child, Duration::from_millis(50)));
        assert!(kill_and_reap(&mut child));
    }

    #[test]
    fn reap_within_reports_true_once_the_child_has_ended() {
        let mut child = Command::new("true").spawn().expect("spawn true");
        assert!(reap_within(&mut child, Duration::from_secs(5)));
    }
}
