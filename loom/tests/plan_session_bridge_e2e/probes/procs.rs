//! The processes a probe leaves: the session's process tree from `/proc`, its forced end, and
//! the watchdog that ends everything when a probe overruns its deadline.

use std::fs;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

/// A process by pid and start time, so a reused pid is never signalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    start: u64,
}

/// `(ppid, start time, state)` from `/proc/<pid>/stat`; the command name may hold spaces and
/// parentheses, so the fields are read after its last `)`.
fn stat(pid: u32) -> Option<(u32, u64, char)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &text[text.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let state = fields.first()?.chars().next()?;
    Some((
        fields.get(1)?.parse().ok()?,
        fields.get(19)?.parse().ok()?,
        state,
    ))
}

/// Whether `proc` still runs: the same pid and start time, and not a zombie.
pub fn alive(proc: &Proc) -> bool {
    stat(proc.pid).is_some_and(|(_, start, state)| start == proc.start && state != 'Z')
}

/// `root` and every live descendant.
pub fn tree(root: u32) -> Vec<Proc> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let pids = entries.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok());
    let all: Vec<(u32, u32, u64)> = pids
        .filter_map(|pid| stat(pid).map(|(ppid, start, _)| (pid, ppid, start)))
        .collect();
    let mut found: Vec<Proc> = all
        .iter()
        .filter(|(pid, ..)| *pid == root)
        .map(|&(pid, _, start)| Proc { pid, start })
        .collect();
    let mut at = 0;
    while at < found.len() {
        let parent = found[at].pid;
        let children = all.iter().filter(|(_, ppid, _)| *ppid == parent);
        found.extend(children.map(|&(pid, _, start)| Proc { pid, start }));
        at += 1;
    }
    found
}

/// The command line of `pid`, its words joined by spaces.
pub fn cmdline(pid: u32) -> String {
    let bytes = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let words = bytes.split(|b| *b == 0).filter(|w| !w.is_empty());
    let words: Vec<String> = words
        .map(|w| String::from_utf8_lossy(w).into_owned())
        .collect();
    words.join(" ")
}

/// Whether `line`, a command line, is a `sleep`, `timeout` or `tail` process running `sleep`
/// or `tail -f`. The sandbox wrapper and the shells around them also name `sleep 45` in an
/// argument; those lines are long, and the wrapper's carries the session's environment.
pub fn sleeper(line: &str) -> bool {
    let program = line.split_whitespace().next().unwrap_or_default();
    let name = program.rsplit('/').next().unwrap_or_default();
    matches!(name, "sleep" | "timeout" | "tail")
        && (line.contains("sleep") || line.contains("tail -f"))
}

/// SIGKILLs each process of `procs` still alive, then waits up to 5 s for them to go; returns
/// those still running. Processes that are not this test's children are reaped by their new
/// parent once killed: only the side process is this test's own child, reaped by its guard.
pub fn kill_all(procs: &[Proc]) -> Vec<Proc> {
    for proc in procs.iter().filter(|p| alive(p)) {
        let _ = kill(Pid::from_raw(proc.pid as i32), Signal::SIGKILL);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let left: Vec<Proc> = procs.iter().copied().filter(alive).collect();
        if left.is_empty() || Instant::now() >= deadline {
            return left;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Kills the tmux server on `socket`.
pub fn kill_server(socket: &str) {
    let _ = Command::new("tmux")
        .args(["-L", socket, "kill-server"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The processes a probe has seen: the session's tree, sampled while it lives (a background
/// task orphaned by a dead parent can no longer be found from it), and the side processes.
#[derive(Clone, Default)]
pub struct Seen {
    pub root: Arc<AtomicU32>,
    pub procs: Arc<Mutex<Vec<Proc>>>,
}

impl Seen {
    pub fn add(&self, found: impl IntoIterator<Item = Proc>) {
        let mut procs = self.procs.lock().unwrap_or_else(PoisonError::into_inner);
        for proc in found {
            if !procs.contains(&proc) {
                procs.push(proc);
            }
        }
    }

    /// Adds the session's live tree, once its root pid is known.
    pub fn sample(&self) {
        let root = self.root.load(Ordering::SeqCst);
        if root != 0 {
            self.add(tree(root));
        }
    }

    pub fn all(&self) -> Vec<Proc> {
        self.procs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Samples the session's tree every 200 ms, and at the deadline ends the tmux server and every
/// process seen, so a blocked call returns; disarmed when the probe ends first.
pub struct Watchdog {
    done: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Watchdog {
    pub fn arm(deadline: Instant, socket: String, seen: Seen) -> Watchdog {
        let done = Arc::new(AtomicBool::new(false));
        let quit = done.clone();
        let thread = thread::spawn(move || {
            while !quit.load(Ordering::SeqCst) {
                seen.sample();
                if Instant::now() >= deadline {
                    eprintln!("probe deadline passed: killing tmux server {socket}");
                    kill_server(&socket);
                    kill_all(&seen.all());
                    return;
                }
                thread::sleep(Duration::from_millis(200));
            }
        });
        Watchdog {
            done,
            thread: Some(thread),
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// `pid` as a [`Proc`], when it runs.
pub fn proc_of(pid: u32) -> Option<Proc> {
    stat(pid).map(|(_, start, _)| Proc { pid, start })
}

#[cfg(test)]
mod tests {
    use super::sleeper;

    #[test]
    fn only_the_sleeping_processes_are_listed_never_a_wrapper_with_an_environment() {
        assert!(sleeper("sleep 45"));
        assert!(sleeper("/usr/bin/sleep 45"));
        assert!(sleeper("timeout 45 tail -f /dev/null"));
        assert!(!sleeper("tail -n 3 notes.txt"));
        assert!(!sleeper(
            "bwrap --setenv TOKEN secret -- /bin/zsh -c sleep 45; echo"
        ));
        assert!(!sleeper("/bin/zsh -c eval 'sleep 45; echo BASH-DONE'"));
        assert!(!sleeper(""));
    }
}
