//! Spawn helper that rides out the Linux `ETXTBSY` fork/exec race.

use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

/// `Command::spawn` briefly, and rarely, fails with `ETXTBSY` ("text file
/// busy") when the target binary is still settling right after being
/// written - e.g. a `codex` self-update in progress, or (in tests) a fake
/// script written moments earlier while another thread forks. A handful of
/// short retries clears it without treating it as a real spawn failure.
pub fn spawn_retrying_text_busy(command: &mut Command) -> std::io::Result<Child> {
    const MAX_ATTEMPTS: u32 = 5;
    const RETRY_DELAY: Duration = Duration::from_millis(20);

    for attempt in 1..=MAX_ATTEMPTS {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(e) if attempt < MAX_ATTEMPTS && e.raw_os_error() == Some(libc::ETXTBSY) => {
                thread::sleep(RETRY_DELAY);
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop always returns on its final attempt")
}
