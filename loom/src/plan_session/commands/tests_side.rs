//! The side process against fake `claude` scripts: the answer and a `commands_changed` line,
//! the output cap, the end of stdout before the answer, `stop`, drop, and a symlinked settings
//! path.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use serde_json::{json, Value};

use super::list::{CatalogueState, CommandTable};
use super::side::{side_argv, side_settings, OnTable, SideProcess, SideSpec};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
);
const WAIT: Duration = Duration::from_secs(10);

fn alive(pid: u32) -> bool {
    let pid = Pid::from_raw(i32::try_from(pid).unwrap());
    kill(pid, None) != Err(Errno::ESRCH)
}

/// A spec running `body` as a `/bin/sh` script in `base`, settings at `<base>/side.json`.
fn spec(base: &Path, body: &str) -> SideSpec {
    let script = base.join("claude.sh");
    fs::write(&script, format!("#!/bin/sh\n{body}")).unwrap();
    SideSpec {
        program: PathBuf::from("/bin/sh"),
        args: vec![script.into_os_string()],
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        cwd: base.to_path_buf(),
        settings_path: base.join("side.json"),
        settings: json!({"disableAllHooks": true, "enabledPlugins": {"loom-ui@skills-dir": false}}),
    }
}

fn channel() -> (OnTable, mpsc::Receiver<Arc<CommandTable>>) {
    let (tx, rx) = mpsc::channel();
    let on_table: OnTable = Box::new(move |table| {
        let _ = tx.send(table);
    });
    (on_table, rx)
}

fn base() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    (temp, base)
}

/// A fake that logs its stdin, answers with fixture line 1 under the id it was sent, prints
/// line 2, then copies the rest of its stdin to the log.
fn answering_body(base: &Path) -> String {
    let fixture = fs::read_to_string(FIXTURE).unwrap();
    let lines: Vec<&str> = fixture.lines().collect();
    fs::write(base.join("answer.jsonl"), format!("{}\n", lines[0])).unwrap();
    fs::write(base.join("changed.jsonl"), format!("{}\n", lines[1])).unwrap();
    let (log, answer, changed) = (
        base.join("stdin.log"),
        base.join("answer.jsonl"),
        base.join("changed.jsonl"),
    );
    format!(
        "IFS= read -r line\nprintf '%s\\n' \"$line\" > '{log}'\n\
         id=$(printf '%s' \"$line\" | sed 's/.*\"request_id\":\"\\([^\"]*\\)\".*/\\1/')\n\
         sed \"s/probe-init-1/$id/\" '{answer}'\ncat '{changed}'\ncat >> '{log}'\n",
        log = log.display(),
        answer = answer.display(),
        changed = changed.display(),
    )
}

fn listed(table: &CommandTable) -> usize {
    table.entries().len() - CommandTable::loading().entries().len()
}

#[test]
fn the_answer_and_a_commands_changed_line_replace_the_table() {
    let (_temp, base) = base();
    let spec = spec(&base, &answering_body(&base));
    let (on_table, rx) = channel();
    let side = SideProcess::start(spec, Duration::from_secs(10), on_table).unwrap();
    let pid = side.pid();

    let first = rx.recv_timeout(WAIT).expect("the answer's table");
    assert_eq!(first.state(), &CatalogueState::Ready { generation: 1 });
    assert_eq!(listed(&first), 81, "117 rows less the 36 native ones");
    let second = rx.recv_timeout(WAIT).expect("the changed table");
    assert_eq!(second.state(), &CatalogueState::Ready { generation: 2 });
    assert!(second.lookup("probe-added").is_some());
    let settings = base.join("side.json");
    let mode = fs::metadata(&settings).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let written: Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(
        written["enabledPlugins"],
        json!({"loom-ui@skills-dir": false})
    );

    side.stop();
    assert!(
        !alive(pid),
        "stop left the side process running or unreaped"
    );
    assert!(!settings.exists(), "stop left the settings file");
    let log = fs::read_to_string(base.join("stdin.log")).unwrap();
    assert_eq!(log.lines().count(), 1, "{log:?}");
    let request: Value = serde_json::from_str(log.trim_end()).unwrap();
    assert_eq!(request["request"], json!({"subtype": "initialize"}));
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
}

fn assert_unavailable_once(rx: &mpsc::Receiver<Arc<CommandTable>>, pid: u32, base: &Path) {
    let table = rx.recv_timeout(WAIT).expect("a table");
    let unavailable = matches!(table.state(), CatalogueState::Unavailable { .. });
    assert!(unavailable, "{:?}", table.state());
    assert_eq!(listed(&table), 0);
    assert!(!alive(pid), "the side process is still running or unreaped");
    assert!(
        !base.join("side.json").exists(),
        "the settings file is left"
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "a second table"
    );
}

#[test]
fn a_line_over_the_cap_kills_the_process() {
    let (_temp, base) = base();
    let body = "head -c 2097152 /dev/zero | tr '\\0' x\necho\nexec sleep 30\n";
    let (on_table, rx) = channel();
    let side = SideProcess::start(spec(&base, body), Duration::from_secs(10), on_table).unwrap();
    assert_unavailable_once(&rx, side.pid(), &base);
}

#[test]
fn over_8_mib_of_short_lines_before_the_answer_kills_the_process() {
    let (_temp, base) = base();
    let body = "yes 0123456789abcdef | head -c 9437184\nexec sleep 30\n";
    let (on_table, rx) = channel();
    let side = SideProcess::start(spec(&base, body), Duration::from_secs(10), on_table).unwrap();
    let (pid, table) = (side.pid(), rx.recv_timeout(WAIT).expect("a table"));
    let reason = "The command list process printed over 8 MiB before its answer".to_owned();
    assert_eq!(table.state(), &CatalogueState::Unavailable { reason });
    assert!(!alive(pid), "the side process is still running or unreaped");
    assert!(
        !base.join("side.json").exists(),
        "the settings file is left"
    );
}

#[test]
fn the_end_of_stdout_before_the_answer_is_unavailable_once() {
    let (_temp, base) = base();
    let (on_table, rx) = channel();
    let spec = spec(&base, "exit 0\n");
    let side = SideProcess::start(spec, Duration::from_secs(10), on_table).unwrap();
    assert_unavailable_once(&rx, side.pid(), &base);
}

#[test]
fn drop_kills_and_reaps_a_silent_process() {
    let (_temp, base) = base();
    let (on_table, rx) = channel();
    let spec = spec(&base, "exec sleep 30\n");
    let side = SideProcess::start(spec, Duration::from_secs(30), on_table).unwrap();
    let pid = side.pid();
    assert!(alive(pid));
    drop(side);
    assert!(
        !alive(pid),
        "drop left the side process running or unreaped"
    );
    assert!(!base.join("side.json").exists());
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "a table after drop"
    );
}

#[test]
fn a_symlinked_settings_path_is_refused() {
    let (_temp, base) = base();
    let target = base.join("target.json");
    fs::write(&target, "untouched").unwrap();
    symlink(&target, base.join("side.json")).unwrap();
    let (on_table, _rx) = channel();
    let started = SideProcess::start(spec(&base, "exit 0\n"), Duration::from_secs(5), on_table);
    assert!(started.is_err(), "a symlinked settings path was followed");
    assert_eq!(fs::read_to_string(&target).unwrap(), "untouched");
}

fn words(text: &str) -> Vec<OsString> {
    text.split(' ').map(OsString::from).collect()
}

#[test]
fn side_argv_keeps_flag_values_that_look_like_dropped_flags() {
    let session = words(
        "claude --session-id s --name --settings --settings /s.json --model --name \
         --plugin-dir /p --strict-mcp-config --append-system-prompt-file /b.md",
    );
    let want = words(
        "claude --settings /side.json --model --name --plugin-dir /p --strict-mcp-config -p \
         --input-format stream-json --output-format stream-json --verbose \
         --no-session-persistence",
    );
    assert_eq!(side_argv(&session, Path::new("/side.json")), want);
}

#[test]
fn side_settings_drop_only_the_hooks() {
    let session = json!({"hooks": {"Stop": []}, "sandbox": {"enabled": true},
                         "enabledPlugins": {"loom-ui@skills-dir": false}});
    let want = json!({"sandbox": {"enabled": true}, "disableAllHooks": true,
                      "enabledPlugins": {"loom-ui@skills-dir": false}});
    assert_eq!(side_settings(&session), want);
    assert_eq!(
        side_settings(&json!(null)),
        json!({"disableAllHooks": true})
    );
}
