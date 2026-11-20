//! Starting a launcher once to learn whether it can run here.

use super::reap;
use std::path::Path;

/// How long [`start_blocker`] lets the launcher run before it counts as a blocker.
const START_BLOCKER_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// Run `program args..` with a null stdin and wait, at most `START_BLOCKER_DEADLINE`.
/// `None` when it exits 0; otherwise the reason: its exit status and first
/// non-empty stderr line, the spawn error when it cannot be started, or that it
/// outran the deadline (killed and reaped).
pub fn start_blocker(program: &Path, args: &[&str]) -> Option<String> {
    let name = program.display();
    let spawned = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Some(format!("{name} not found"));
        }
        Err(error) => return Some(format!("{name} could not be started ({error})")),
    };
    let status = match wait_for_launcher(&mut child) {
        Ok(status) => status,
        Err(reason) => return Some(format!("{name} {reason}")),
    };
    if status.success() {
        return None;
    }
    Some(format!(
        "{name} failed ({status}: {})",
        first_stderr_line(&mut child)
    ))
}

/// Polls the launcher until it ends; past the deadline, or when it cannot be
/// waited on, it is killed and reaped (bounded) and the reason returned.
fn wait_for_launcher(child: &mut std::process::Child) -> Result<std::process::ExitStatus, String> {
    let deadline = std::time::Instant::now() + START_BLOCKER_DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                reap::kill_and_reap(child);
                return Err(format!("did not finish within {START_BLOCKER_DEADLINE:?}"));
            }
            Err(error) => {
                reap::kill_and_reap(child);
                return Err(format!("could not be waited on ({error})"));
            }
        }
    }
}

/// The first non-empty line the ended launcher wrote to stderr, or `no stderr`.
fn first_stderr_line(child: &mut std::process::Child) -> String {
    use std::io::Read;
    let mut bytes = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no stderr")
        .to_string()
}
