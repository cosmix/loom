//! Stand-in processes a test signals: a direct child, the leader of a process
//! group of its own (with whatever tree it starts, such as a supervisor-named
//! shell over a busy grandchild), or an orphan reparented away from the test.
//! A process or group the code under test started is adopted the same way.
//!
//! A host job's sandbox can refuse every signal. A test that needs one starts
//! with `require_signals!()` or `require_orphan_signals!()`, which return from
//! it there, or fail it under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`; a constructor
//! called without the guard's probe passing panics instead of spawning a
//! process the test could not remove. Cleanup checks the kill and waits with
//! a deadline: a refused kill or a survivor fails the test with the pid and
//! the error, and never blocks it. A direct child is signalled only while
//! `Child` has not reaped it, so its pid cannot have been given to another
//! process; a group only while `waitid` finds its leader unreaped, which a
//! later child of this process given the pid of a leader the test reaped
//! would also pass; an orphan or an adopted group only while its start time
//! still matches (see `adopted`). When that cannot be told, cleanup signals
//! nothing and fails the test instead.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::{kill, killpg, Signal};
use nix::unistd::Pid;

use crate::process::sandbox_probe::{
    process_groups_signalable, skip_without_group_signals, skip_without_orphan_signals,
};
use crate::process::wait_with_deadline;

mod adopted;

pub(crate) use adopted::{orphan, orphan_running, AdoptedGroup, Orphan};

/// How long cleanup waits for a killed stand-in to disappear.
const CLEANUP_BOUND: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

/// The running test, as libtest names its thread.
fn test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("a stand-in test")
        .to_string()
}

/// True when the test must return now: this host refuses to kill a child's
/// process group, so a [`StandIn`] could not be stopped. Prints a `SKIP` line,
/// or panics under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`.
pub(crate) fn skip_without_signals() -> bool {
    skip_without_group_signals(&test_name())
}

/// True when the test must return now: this host refuses to kill a process
/// group or a process reparented away from the test, so an [`Orphan`] could
/// not be stopped, nor could loom retire it.
pub(crate) fn skip_without_orphans() -> bool {
    skip_without_orphan_signals(&test_name())
}

/// Return from the calling test when [`skip_without_signals`] says to skip it.
macro_rules! require_signals {
    () => {
        if $crate::test_stand_in::skip_without_signals() {
            return;
        }
    };
}
pub(crate) use require_signals;

/// Return from the calling test when [`skip_without_orphans`] says to skip it.
macro_rules! require_orphan_signals {
    () => {
        if $crate::test_stand_in::skip_without_orphans() {
            return;
        }
    };
}
pub(crate) use require_orphan_signals;

/// What a stand-in's cleanup signals.
#[derive(Clone, Copy, Debug)]
enum Reach {
    /// The direct child alone.
    Child,
    /// Every process in the group the child leads.
    Group,
}

impl Reach {
    fn noun(self) -> &'static str {
        match self {
            Reach::Child => "process",
            Reach::Group => "process group",
        }
    }
}

/// A child process the test owns, killed with `SIGKILL` and reaped on drop.
pub(crate) struct StandIn {
    child: Child,
    reach: Reach,
}

impl StandIn {
    /// Spawn `command` as the leader of a new process group. Cleanup kills the
    /// whole group, so a grandchild that keeps running is removed with it,
    /// provided the leader is still unreaped then: once the test reaps it
    /// (through [`StandIn::wait_within`] or [`StandIn::child`]), the group id
    /// may name another group, and cleanup only reports a group still there.
    pub(crate) fn group_leader(command: &mut Command) -> Self {
        Self::spawn(command.process_group(0), Reach::Group)
    }

    /// Spawn `command` as the leader of a new session, and so of a new process
    /// group, as a boundary's agent runs. Cleanup kills the group as
    /// [`StandIn::group_leader`]'s does. Only the macOS boundary tests start
    /// one, so other platforms do not compile it.
    #[cfg(target_os = "macos")]
    pub(crate) fn session_leader(command: &mut Command) -> Self {
        // SAFETY: `setsid` is async-signal-safe and touches no parent memory.
        unsafe {
            command.pre_exec(|| match libc::setsid() {
                -1 => Err(std::io::Error::last_os_error()),
                _ => Ok(()),
            })
        };
        Self::spawn(command, Reach::Group)
    }

    /// Spawn `command` as a direct child in the test's own process group.
    /// Cleanup kills the child alone.
    pub(crate) fn direct(command: &mut Command) -> Self {
        Self::spawn(command, Reach::Child)
    }

    /// Spawn `command`, which ends on its own, as a direct child. A test that
    /// sends it no signal needs no `require_signals!()`: cleanup kills only a
    /// child still running, and a refused kill fails the test.
    pub(crate) fn exits_alone(command: &mut Command) -> Self {
        Self::launch(command, Reach::Child)
    }

    fn spawn(command: &mut Command, reach: Reach) -> Self {
        assert!(
            process_groups_signalable(),
            "{}: a stand-in was spawned without require_signals!() first",
            test_name()
        );
        Self::launch(command, reach)
    }

    fn launch(command: &mut Command, reach: Reach) -> Self {
        let child = command
            .spawn()
            .unwrap_or_else(|error| panic!("spawn stand-in {command:?}: {error}"));
        Self { child, reach }
    }

    pub(crate) fn id(&self) -> u32 {
        self.child.id()
    }

    /// The child, for its pipes and `try_wait`. Wait through
    /// [`StandIn::wait_within`]: a bare `wait()` blocks for as long as a
    /// stand-in that missed its signal runs.
    pub(crate) fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Reap the child, failing the test when it is still running after `bound`.
    pub(crate) fn wait_within(&mut self, bound: Duration) -> ExitStatus {
        let pid = self.child.id();
        match wait_with_deadline(&mut self.child, bound) {
            Ok(Some(status)) => status,
            Ok(None) => panic!("stand-in {pid} was still running {bound:?} later"),
            Err(error) => panic!("reap stand-in {pid}: {error}"),
        }
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        settle(retire(
            &mut self.child,
            self.reach,
            send_kill,
            CLEANUP_BOUND,
        ));
    }
}

fn send_kill(reach: Reach, target: Pid) -> nix::Result<()> {
    match reach {
        Reach::Child => kill(target, Signal::SIGKILL),
        Reach::Group => killpg(target, Signal::SIGKILL),
    }
}

fn target_of(pid: u32) -> Pid {
    Pid::from_raw(i32::try_from(pid).expect("a pid fits in i32"))
}

/// Whether `pid`, a child of this process, is still unreaped: running, or a
/// zombie. Reads without reaping (`WNOWAIT`). While it is unreaped its pid,
/// and the id of a group it leads, cannot be given to another process.
/// `ECHILD` means it was reaped; any other error leaves that unknown. Once
/// it was reaped, a later child of this process given the same pid reads as
/// unreaped too: `waitid` names a pid, not a process.
fn unreaped(pid: u32) -> nix::Result<bool> {
    let id = libc::id_t::from(pid);
    loop {
        // SAFETY: an all-zero `siginfo_t` is a valid value for the call to
        // overwrite, and `info` outlives the call.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let flags = libc::WEXITED | libc::WNOHANG | libc::WNOWAIT;
        // SAFETY: `info` is a valid, writable `siginfo_t`; WNOWAIT leaves the
        // child waitable, so `Child` still reaps it.
        let status = unsafe { libc::waitid(libc::P_PID, id, &mut info, flags) };
        if status == 0 {
            return Ok(true);
        }
        match Errno::last() {
            Errno::EINTR => continue,
            Errno::ECHILD => return Ok(false),
            errno => return Err(errno),
        }
    }
}

/// Whether `child` still holds its pid, and so the id of a group it leads.
fn pinned(child: &mut Child, reach: Reach) -> Result<bool, String> {
    let pid = child.id();
    let held = match reach {
        // `Child` records its own reap, so a pid it gave up is never signalled.
        Reach::Child => child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|e| e.to_string()),
        // Reaping a leader frees its group id, so it is read without reaping.
        Reach::Group => unreaped(pid).map_err(|errno| errno.to_string()),
    };
    held.map_err(|error| {
        format!(
            "stand-in {pid}: could not tell whether it was reaped ({error}); its {} was not \
             signalled and may still be running",
            reach.noun()
        )
    })
}

/// Kill `child`, or the group it leads, through `send`, then wait at most
/// `bound` until it is gone. `send` is the real kill outside this module's tests.
fn retire(
    child: &mut Child,
    reach: Reach,
    send: impl Fn(Reach, Pid) -> nix::Result<()>,
    bound: Duration,
) -> Result<(), String> {
    let pid = child.id();
    let target = target_of(pid);
    let group_gone = || killpg(target, None) == Err(Errno::ESRCH);
    let deadline = Instant::now() + bound;
    if !pinned(child, reach)? {
        // The test reaped the leader: the id may name someone else's group now,
        // so it is probed with the null signal and never killed. A member that
        // just died stays in the group until its new parent reaps it.
        return match reach {
            Reach::Child => Ok(()),
            Reach::Group => {
                await_gone(pid, reach.noun(), deadline, bound, group_gone).map_err(|_| {
                    format!(
                        "stand-in {pid}: its process group outlived the leader the test reaped; \
                         not signalled, since the id may have been reused"
                    )
                })
            }
        };
    }
    let sent = send(reach, target);
    // macOS refuses a group kill with EPERM when every member is a zombie, as
    // a leader that exited on its own is until it is reaped below. A live
    // member that refused the kill keeps the group answering, so the wait for
    // the group to go fails instead.
    let only_zombies = matches!((reach, sent), (Reach::Group, Err(Errno::EPERM)))
        && crate::process::process_is_zombie(pid);
    if !only_zombies {
        refused(pid, reach.noun(), sent)?;
    }
    reap_killed(child, reach.noun(), bound)?;
    match reach {
        Reach::Child => Ok(()),
        Reach::Group if only_zombies => await_gone(pid, reach.noun(), deadline, bound, group_gone)
            .map_err(|_| refused_with_zombie_leader(pid, bound)),
        Reach::Group => await_gone(pid, reach.noun(), deadline, bound, group_gone),
    }
}

/// The failure for a group whose SIGKILL was refused with EPERM while its
/// leader was a zombie, yet still answered once the leader was reaped.
fn refused_with_zombie_leader(pid: u32, bound: Duration) -> String {
    format!(
        "stand-in {pid}: SIGKILL to its process group was refused (EPERM) while its leader was \
         a zombie, and the group still answered {bound:?} later; a live member may still be \
         running"
    )
}

/// Reap `child` after its kill, waiting at most `bound`.
fn reap_killed(child: &mut Child, noun: &str, bound: Duration) -> Result<(), String> {
    let pid = child.id();
    match wait_with_deadline(child, bound) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(still_running(pid, noun, bound)),
        Err(error) => Err(format!(
            "stand-in {pid}: could not reap it after SIGKILL to its {noun} ({error}); it may \
             still be running"
        )),
    }
}

/// `Err` naming `pid` when the kill was refused; a target already gone is fine.
fn refused(pid: u32, noun: &str, sent: nix::Result<()>) -> Result<(), String> {
    match sent {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(errno) => Err(format!(
            "stand-in {pid}: SIGKILL to its {noun} was refused ({errno}); it may still be running"
        )),
    }
}

fn still_running(pid: u32, noun: &str, bound: Duration) -> String {
    format!("stand-in {pid}: its {noun} was still running {bound:?} after SIGKILL")
}

/// Poll `gone` until it holds; `Err` once `deadline` passes, naming `bound`.
/// A killed group member is gone only once its parent, or init, reaps it.
fn await_gone(
    pid: u32,
    noun: &str,
    deadline: Instant,
    bound: Duration,
    mut gone: impl FnMut() -> bool,
) -> Result<(), String> {
    while !gone() {
        if Instant::now() >= deadline {
            return Err(still_running(pid, noun, bound));
        }
        std::thread::sleep(POLL);
    }
    Ok(())
}

/// Fail the test with `outcome`'s error, unless it is already unwinding from
/// a failure: a second panic there would abort the whole test binary.
fn settle(outcome: Result<(), String>) {
    let Err(why) = outcome else {
        return;
    };
    if std::thread::panicking() {
        eprintln!("stand-in cleanup failed while the test was already failing: {why}");
    } else {
        panic!("{why}");
    }
}

/// The pid of a child that exited and was reaped: no process holds it until
/// the kernel gives the number out again.
pub(crate) fn reaped_pid() -> u32 {
    let mut child = Command::new("true")
        .spawn()
        .unwrap_or_else(|error| panic!("spawn true: {error}"));
    let pid = child.id();
    let reaped = wait_with_deadline(&mut child, CLEANUP_BOUND);
    assert!(matches!(reaped, Ok(Some(_))), "reap true {pid}: {reaped:?}");
    pid
}

#[path = "test_stand_in_tests.rs"]
mod tests;
