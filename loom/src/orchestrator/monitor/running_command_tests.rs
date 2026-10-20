//! Telling a Bash tool call apart from the children a Claude process always
//! keeps, and reading both process-table formats that name them.

use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};

use super::*;
use crate::models::session::SessionType;
use crate::orchestrator::terminal::native::create_wrapper_script;
use crate::process::OwnedProcess;

/// The agent's pid in the synthetic process tables below.
const AGENT: u32 = 100;

fn row(pid: u32, ppid: u32, name: &str) -> ProcessRow {
    ProcessRow {
        pid,
        ppid,
        zombie: false,
        name: name.to_string(),
    }
}

/// Every process in the synthetic tables started after the agent.
fn later_than_agent(pid: u32) -> Option<u64> {
    Some(if pid == AGENT { 1 } else { 2 })
}

/// A tool call's shell runs something; a shell with nothing under it, or with
/// only the wrapper's `tee` under it, is the wrapper's own leftover.
#[test]
fn a_launcher_counts_only_while_it_runs_something_other_than_tee() {
    let tee_parent = [row(101, AGENT, "bash"), row(102, 101, "tee")];
    assert!(first_command(AGENT, &tee_parent, later_than_agent).is_none());

    let idle = [row(101, AGENT, "bash")];
    assert!(first_command(AGENT, &idle, later_than_agent).is_none());

    let mut finished = [row(101, AGENT, "bash"), row(102, 101, "sleep")];
    finished[1].zombie = true;
    assert!(first_command(AGENT, &finished, later_than_agent).is_none());

    let running = [row(101, AGENT, "bash"), row(102, 101, "loom")];
    assert_eq!(
        first_command(AGENT, &running, later_than_agent)
            .unwrap()
            .pid,
        101
    );
}

/// A launcher that did not start after the agent was forked before the
/// wrapper's `exec`; without start times the other rules decide alone.
#[test]
fn a_launcher_must_start_after_the_agent_when_both_starts_are_known() {
    let rows = [row(101, AGENT, "sh"), row(102, 101, "sleep")];
    assert!(first_command(AGENT, &rows, |_| Some(7)).is_none());
    assert!(first_command(AGENT, &rows, |_| None).is_some());
    let agent_unknown = |pid| (pid != AGENT).then_some(7);
    assert!(first_command(AGENT, &rows, agent_unknown).is_some());
}

/// Children observed under live sessions for their whole run: the wrapper's
/// stderr `tee`, the sandbox proxy bridge, an MCP server, the macOS
/// sleep guard. Counting any of them would hold every judge open.
const ALWAYS_THERE: &[&str] = &["tee", "socat", "npm exec @playw", "node", "caffeinate"];

/// Only the shell running a tool call, or the sandbox launcher wrapping it,
/// is a command.
#[test]
fn only_a_shell_or_its_sandbox_launcher_is_a_command() {
    for launcher in ["bash", "zsh", "sh", "bwrap", "sandbox-exec"] {
        assert!(is_command_launcher(launcher), "{launcher} runs tool calls");
    }
    for name in ALWAYS_THERE {
        assert!(!is_command_launcher(name), "{name} is not a command");
    }
}

/// `ps` on macOS names the executable by path, and a login shell by `-zsh`.
#[test]
fn a_path_or_login_shell_name_is_matched_by_its_executable() {
    assert!(is_command_launcher("/bin/zsh"));
    assert!(is_command_launcher("-zsh"));
    assert!(is_command_launcher("/usr/bin/sandbox-exec"));
    assert!(!is_command_launcher("/usr/bin/tee"));
    assert!(!is_command_launcher("/opt/bash-tools/server"));
}

/// A name with spaces and parens must not shift the fields after it: the
/// state and parent are read after the LAST ") ".
#[test]
fn a_proc_stat_line_is_read_after_the_last_paren() {
    let stat = "42 (odd) name) S 7 42 42 0 -1";
    let row = parse_proc_stat(42, stat).unwrap();
    assert_eq!(
        row,
        ProcessRow {
            pid: 42,
            ppid: 7,
            zombie: false,
            name: "odd) name".to_string(),
        }
    );
    assert!(parse_proc_stat(9, "9 (zsh) Z 7 9 9 0 -1").unwrap().zombie);
    assert!(parse_proc_stat(9, "garbage").is_none());
}

/// `ps` right-aligns the numeric columns, and a macOS path may hold spaces.
#[test]
fn a_ps_row_keeps_the_whole_name() {
    let line = "  4242   17 S+   /Applications/My App.app/run";
    let row = parse_ps_row(line).unwrap();
    assert_eq!(
        row,
        ProcessRow {
            pid: 4242,
            ppid: 17,
            zombie: false,
            name: "/Applications/My App.app/run".to_string(),
        }
    );
    assert!(parse_ps_row("5 1 Z <defunct>").unwrap().zombie);
    assert!(parse_ps_row("5 1 S").is_none());
    assert!(parse_ps_row("PID PPID STAT COMM").is_none());
}

/// Block until `ready()` holds, failing the test after ten seconds.
fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A stand-in for claude, run by the real wrapper: it says it is up, idles
/// until told to go, then runs a tool call the way Claude Code does — a shell
/// child with a command of its own — and marks it started.
fn stub_claude(dir: &Path) -> String {
    let path = |name: &str| format!("'{}'", dir.join(name).display());
    format!(
        "touch {ready}\n\
         while [ ! -e {go} ]; do sleep 0.05; done\n\
         sh -c 'sleep 30 & touch \"$1\"; wait' tool-call {started}\n\
         :\n",
        ready = path("ready"),
        go = path("go"),
        started = path("started"),
    )
}

/// Run the REAL generated session wrapper over [`stub_claude`] for
/// `session`, in its own process group so dropping the guard ends it and
/// everything it started.
fn run_wrapped_stub(dir: &Path, session: &Session, pid_key: &str) -> OwnedProcess {
    let stub = dir.join("claude-stub.sh");
    std::fs::write(&stub, stub_claude(dir)).unwrap();
    let claude_cmd = format!("/bin/sh '{}'", stub.display());
    let stage_id = session.stage_id.as_deref().unwrap();
    let wrapper = create_wrapper_script(
        dir,
        pid_key,
        stage_id,
        &session.id,
        &claude_cmd,
        None,
        SessionType::Adjudication,
        150_000,
    )
    .unwrap();
    // `bash <script>` reads the script instead of executing the file just
    // written, so a sibling test's fork cannot fail the spawn with ETXTBSY.
    let child = std::process::Command::new("bash")
        .arg(&wrapper)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .expect("failed to run the session wrapper");
    OwnedProcess::new(child)
}

/// Whether the wrapper's `tee` runs under `agent`, directly or under the
/// subshell its process substitution forked.
fn wrapper_tee_runs_under(agent: u32) -> bool {
    let rows = process_rows();
    let parent_of = |pid| rows.iter().find(|row| row.pid == pid).map(|row| row.ppid);
    rows.iter().any(|row| {
        executable(&row.name) == WRAPPER_TEE
            && (row.ppid == agent || parent_of(row.ppid) == Some(agent))
    })
}

/// The session wrapper starts its stderr `tee` through a process substitution
/// forked before `exec`, so claude inherits it — possibly under a `bash`
/// subshell. With the real wrapper, while the stand-in only idles the
/// wrapper's own children must not read as a command; once it starts a tool
/// call, that must.
#[test]
fn the_session_wrapper_reads_as_a_command_only_while_a_tool_call_runs() {
    let temp = tempfile::TempDir::new().unwrap();
    let dir = temp.path();
    let mut session = Session::new_adjudication("wrapped-stage");
    session.id = "judge-wrapped-1".to_string();
    let (_, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
    let _wrapper = run_wrapped_stub(dir, &session, &pid_key);

    wait_until("the stand-in to start", || dir.join("ready").exists());
    let agent = read_pid_entry(dir, &pid_key)
        .expect("the wrapper writes the PID file")
        .pid;
    wait_until("the wrapper's tee", || wrapper_tee_runs_under(agent));
    for _ in 0..5 {
        let command = running_command(dir, &session);
        assert!(
            command.is_none(),
            "an idle agent reads as running {command:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    std::fs::write(dir.join("go"), "").unwrap();
    wait_until("the tool call", || dir.join("started").exists());
    let command = running_command(dir, &session).expect("the tool call's shell is a command");
    assert!(is_command_launcher(&command.name), "{command:?}");
}
