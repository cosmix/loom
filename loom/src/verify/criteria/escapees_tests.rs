use super::*;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::models::stage::CommandConfinement;
use crate::verify::criteria::{
    run_spec_with_timeout, set_spawn_observer, CommandSpec, GroupAnchor, SpawnObserver,
};

const CHILD_ENV: &str = "LOOM_ESCAPEES_TEST_COPY";
/// Longest a copy of the test binary may run.
const COPY_DEADLINE: Duration = Duration::from_secs(60);

fn entry(pid: u32, ppid: u32, pgid: u32, sid: u32) -> ProcessEntry {
    ProcessEntry {
        pid,
        ppid,
        pgid,
        sid,
    }
}

fn pids(found: Vec<ProcessEntry>) -> Vec<u32> {
    found.into_iter().map(|entry| entry.pid).collect()
}

fn leader(pid: u32, anchor: Option<u32>, started: Option<u64>) -> Leader {
    Leader {
        pid,
        anchor,
        started,
    }
}

#[test]
fn the_leaders_session_and_what_its_members_start_are_found() {
    let mut table = vec![
        entry(10, 1, 10, 10),  // me
        entry(20, 10, 20, 20), // leader of its own session
        entry(21, 20, 21, 20), // its child, in a group of its own
        entry(22, 1, 21, 20),  // an orphan in that group
        entry(23, 21, 23, 23), // a session of its own, parent still running
        entry(24, 23, 23, 23), // that session's child
        entry(30, 10, 30, 10), // another child of mine
        entry(31, 1, 31, 31),  // a stranger
    ];
    let any_start = |_: u32| Some(0);
    let leader = leader(20, None, None);
    assert_eq!(
        pids(members(&table, leader, 10, &any_start)),
        [21, 22, 23, 24]
    );
    // A zombie leader is not in the table on macOS; its pid still names the session.
    table.retain(|entry| entry.pid != 20);
    assert_eq!(
        pids(members(&table, leader, 10, &any_start)),
        [21, 22, 23, 24]
    );
}

#[test]
fn in_my_session_only_the_criterions_branches_and_later_orphans_are_found() {
    let table = [
        entry(10, 1, 10, 10),  // me, leading the job's session
        entry(40, 10, 40, 10), // the anchor
        entry(20, 10, 40, 10), // the leader, in the anchor's group
        entry(21, 20, 21, 10), // its child, in a group of its own
        entry(22, 1, 22, 10),  // an orphan that left its group, started after the leader
        entry(25, 1, 25, 10),  // an earlier criterion's orphan, started before it
        entry(26, 25, 25, 10), // a worker that orphan started after the leader
        entry(30, 10, 30, 10), // a helper of mine
        entry(32, 30, 32, 10), // the helper's child in a group of its own
        entry(33, 1, 33, 33),  // a stranger
    ];
    let starts = HashMap::from([(22, 150), (25, 50), (26, 300)]);
    let start = |pid: u32| starts.get(&pid).copied();
    let timed = leader(20, Some(40), Some(100));
    assert_eq!(pids(members(&table, timed, 10, &start)), [21, 22]);
    // Without the leader's start time no orphan is the criterion's.
    let untimed = leader(20, Some(40), None);
    assert_eq!(pids(members(&table, untimed, 10, &start)), [21]);
    // Not leading a session, I find the anchor's group and its branches only.
    let mut unled = table;
    unled.iter_mut().for_each(|entry| entry.sid = 5);
    assert_eq!(pids(members(&unled, timed, 10, &start)), [21]);
}

/// A criterion that leaves a child in a group of its own, and an orphan in a
/// group whose leader exited, each writing its pid into `dir`, then waits.
fn escaping_criterion(dir: &Path) -> CommandSpec {
    let dir = shell_escape::escape(dir.to_string_lossy());
    CommandSpec::shell(format!(
        "cd {dir}; \
         perl -e 'setpgrp(0, 0); exec @ARGV' sh -c 'echo $$ > child.pid; exec sleep 30' & \
         perl -e 'setpgrp(0, 0); exec @ARGV' sh -c '(sleep 30 & echo $! > orphan.pid)'; \
         sleep 30"
    ))
}

/// The processes a copy of the test binary started, by the pids they wrote
/// into `*.pid` files of `dir`, one per line. A pid is taken with its start
/// time only from a complete line and only when its process started after
/// the copy did, so no unrelated process is ever named; on drop every one
/// still running under the same identity is killed.
struct Started {
    dir: PathBuf,
    since: Option<u64>,
    seen: Vec<(String, ProcessIdentity)>,
}

impl Started {
    fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            since: None,
            seen: Vec::new(),
        }
    }

    fn capture(&mut self) {
        let Some(since) = self.since else { return };
        for entry in std::fs::read_dir(&self.dir).unwrap().flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = file.strip_suffix(".pid") else {
                continue;
            };
            let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
            let complete = text.rsplit_once('\n').map_or("", |(lines, _)| lines);
            for pid in complete
                .lines()
                .filter_map(|line| line.trim().parse::<u32>().ok())
            {
                if self.seen.iter().any(|(_, seen)| seen.pid == pid) {
                    continue;
                }
                if let Some(start) = process_start_time(pid).filter(|start| *start >= since) {
                    let identity = ProcessIdentity {
                        pid,
                        start_time: Some(start),
                    };
                    self.seen.push((stem.to_string(), identity));
                }
            }
        }
    }

    fn of(&self, stem: &str) -> Vec<ProcessIdentity> {
        let named = self.seen.iter().filter(|(seen, _)| seen == stem);
        named.map(|(_, identity)| *identity).collect()
    }

    fn running(&self, stems: &[&str]) -> Vec<ProcessIdentity> {
        let all = stems.iter().flat_map(|stem| self.of(stem));
        all.filter(|identity| verify_process_identity(*identity) == IdentityStatus::VerifiedAlive)
            .collect()
    }

    /// Every process of `stems` was seen and is gone within 2 s.
    fn assert_gone(&self, stems: &[&str]) {
        for stem in stems {
            assert!(!self.of(stem).is_empty(), "no {stem}.pid process was seen");
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.running(stems).is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let alive = self.running(stems);
        assert!(alive.is_empty(), "escapees survived the timeout: {alive:?}");
    }
}

impl Drop for Started {
    fn drop(&mut self) {
        let all: Vec<&str> = self.seen.iter().map(|(stem, _)| stem.as_str()).collect();
        for identity in self.running(&all) {
            if let Ok(pid) = i32::try_from(identity.pid) {
                let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
            }
        }
    }
}

fn cannot_signal(name: &str) -> bool {
    crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::process_groups_signalable(),
        name,
        "this process cannot signal a child's process group",
    )
}

/// Run `spec` into its 1 s timeout from `dir`.
fn time_out(spec: &CommandSpec, dir: &Path) {
    let timeout = Duration::from_secs(1);
    let result = run_spec_with_timeout(spec, Some(dir), timeout, CommandConfinement::Inherit);
    let result = result.unwrap();
    assert!(result.timed_out, "{result:?}");
}

/// Run test `name` of this module again in a copy of the test binary, so no
/// other test's spawn observer is installed while it runs, with `dir` in
/// [`CHILD_ENV`]; the copy leads its own session when `session` is set.
/// `started` takes the identity of each process as it writes its pid.
fn run_in_copy(name: &str, dir: &Path, session: bool, started: &mut Started) {
    let log_path = dir.join("copy.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("verify::criteria::escapees::tests::{name}"),
        ])
        .env(CHILD_ENV, dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    if session {
        lead_own_session(&mut command);
    }
    let mut copy = command.spawn().unwrap();
    started.since = process_start_time(copy.id());
    let deadline = Instant::now() + COPY_DEADLINE;
    let status = loop {
        started.capture();
        if let Some(status) = copy.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = copy.kill();
            let _ = copy.wait();
            panic!("the copy ran past {COPY_DEADLINE:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    started.capture();
    let output = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(status.success(), "the copy failed: {output}");
}

#[test]
fn a_timeout_kills_what_left_the_leaders_group() {
    let name = "a_timeout_kills_what_left_the_leaders_group";
    if let Some(dir) = std::env::var_os(CHILD_ENV) {
        let dir = Path::new(&dir);
        return time_out(&escaping_criterion(dir), dir);
    }
    if cannot_signal(name) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut started = Started::new(dir.path());
    run_in_copy(name, dir.path(), false, &mut started);
    started.assert_gone(&["child", "orphan"]);
}

/// The leader's group is stopped before the sweep: a leader that would run
/// the next command of a list, or respawn the worker the sweep killed, waits
/// for its owner to kill it.
#[test]
fn the_leaders_group_is_stopped_before_the_sweep() {
    if cannot_signal("the_leaders_group_is_stopped_before_the_sweep") {
        return;
    }
    let mut command = Command::new("sh");
    command
        .args(["-c", "while :; do sleep 0.05; done"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    lead_own_session(&mut command);
    let mut owner = crate::process::OwnedProcess::new(command.spawn().unwrap());
    let pid = owner.child().id();

    kill_escapees(Leader::spawned(pid, None));

    let state = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    owner.retire();
    let state = String::from_utf8_lossy(&state.stdout);
    assert!(
        state.trim_start().starts_with('T'),
        "the leader runs: {state:?}"
    );
}

/// A host job's child leads its session and runs the criterion in a group
/// anchor's group. An orphan left in that session before the criterion
/// started (by an earlier criterion) is not the criterion's.
#[test]
fn a_host_childs_timeout_kills_what_left_the_anchors_group() {
    let name = "a_host_childs_timeout_kills_what_left_the_anchors_group";
    if let Some(dir) = std::env::var_os(CHILD_ENV) {
        return run_as_host_child(Path::new(&dir));
    }
    if cannot_signal(name) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut started = Started::new(dir.path());
    run_in_copy(name, dir.path(), true, &mut started);
    started.assert_gone(&["child", "orphan"]);
    let earlier = started.of("earlier");
    assert_eq!(earlier.len(), 1, "the earlier orphan was not seen");
    assert_eq!(
        verify_process_identity(earlier[0]),
        IdentityStatus::VerifiedAlive,
        "the earlier orphan was killed"
    );
}

fn run_as_host_child(dir: &Path) {
    let me = nix::unistd::getpid();
    assert_eq!(
        nix::unistd::getsid(None).unwrap(),
        me,
        "not a session leader"
    );
    leave_an_orphan(dir);
    let _observer = set_spawn_observer(SpawnObserver(Box::new(|| {
        use std::os::unix::process::CommandExt;
        let anchor = Command::new("/bin/sleep")
            .arg("600")
            .process_group(0)
            .stdin(Stdio::null())
            .spawn()?;
        Ok(GroupAnchor::new(anchor, Box::new(|| Ok(()))))
    })));
    time_out(&escaping_criterion(dir), dir);
}

/// Leave in this session a process in a group of its own whose parent
/// exited, as a fixture server of an earlier criterion is, and let the clock
/// pass Linux's 10 ms start-time tick before the criterion starts.
fn leave_an_orphan(dir: &Path) {
    let status = Command::new("sh")
        .arg("-c")
        .arg("perl -e 'setpgrp(0, 0); exec @ARGV' sh -c 'echo $$ > earlier.pid; exec sleep 30' &")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dir.join("earlier.pid").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        dir.join("earlier.pid").exists(),
        "the earlier orphan did not start"
    );
    std::thread::sleep(Duration::from_millis(50));
}
