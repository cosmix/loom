//! Orphans: processes a test starts that are reparented away from it, as a
//! session's agent is, and the probe for whether this process may signal one.

use std::ffi::OsStr;
use std::sync::OnceLock;

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use super::{skip_unless, skip_without_group_signals};

/// Start `body` in the background of an `sh` that exits once it has printed
/// the background pid, so the process is reparented away from this one (to
/// PID 1 or the nearest subreaper) and nobody here can reap it. `args` are
/// the shell's `$1`, `$2` and on. Its output goes to `/dev/null`: an
/// inherited stdout would hold open the pipe this reads, until the orphan
/// itself exits.
pub fn spawn_orphan(body: &str, args: &[&OsStr]) -> std::io::Result<u32> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{body} >/dev/null 2>&1 & echo $!"))
        .arg("sh")
        .args(args)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse().map_err(|error| {
        let message = format!("`{body}` printed no orphan pid ({error}): {stdout:?}");
        std::io::Error::new(std::io::ErrorKind::InvalidData, message)
    })
}

/// Whether this process can kill a process it started that was reparented away
/// from it, as the tests that watch loom retire a session's agent need. Distinct
/// from [`super::process_groups_signalable`]: a Seatbelt profile can allow signals to
/// children only (`(target children)`), which passes that probe and refuses
/// this one. The orphan's start time must be readable too: without it a pid
/// cannot be told from a later process given the same number, so it is never
/// signalled. Memoized. A refused probe leaves its `sleep 5` to end on its own.
pub fn orphans_signalable() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| {
        let Ok(pid) = spawn_orphan("sleep 5", &[]) else {
            return false;
        };
        let Ok(raw) = i32::try_from(pid) else {
            return false;
        };
        crate::process::process_start_time(pid).is_some()
            && kill(Pid::from_raw(raw), Signal::SIGKILL).is_ok()
    })
}

/// Skip decision for a test that hands loom an orphan to retire: true when
/// this process cannot signal a child's process group or an orphan, since loom
/// retires an agent through both. Call it before the test spawns anything.
pub fn skip_without_orphan_signals(test_name: &str) -> bool {
    skip_without_group_signals(test_name)
        || skip_unless(
            orphans_signalable(),
            test_name,
            "this process cannot signal, or read the start time of, a process reparented away \
             from it",
        )
}
