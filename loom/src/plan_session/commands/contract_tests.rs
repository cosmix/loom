//! Contracts of the command catalogue: the side process carries neither the session's identity
//! nor its hooks, a side process that never answers is killed, reaped and cleaned up, and every
//! `builtin` row of the `initialize` answer reads as native or bundled.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use serde_json::{json, Value};

use super::list::{CatalogueState, CommandClass, CommandEntry, CommandSource, CommandTable};
use super::side::{side_argv, side_settings, SideProcess, SideSpec};
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::launch::script::claude_argv;
use crate::plan_session::sandbox::{build_settings, ensure_runtime_grants, SandboxInputs};
use crate::planning::paths::LockedSet;
use crate::planning::runtime::runtime_paths_under;

const INITIALIZE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
));
const SESSION_ID: &str = "session-contract-7c1e";
const DISPLAY_NAME: &str = "loom plan: contract-display";
const STREAM_TAIL: [&str; 7] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--no-session-persistence",
];

/// The rows of the recorded `initialize` answer (line 1, `.response.response.commands`).
fn initialize_rows() -> Vec<Value> {
    let first = INITIALIZE
        .lines()
        .next()
        .expect("the initialize answer line");
    let answer: Value = serde_json::from_str(first).expect("the answer is JSON");
    let rows = answer["response"]["response"]["commands"].as_array();
    rows.expect("the answer's command rows").clone()
}

/// A spec as `launch/script.rs`'s test `spec()` builds one.
fn launch_spec(base: &Path) -> LaunchSpec {
    let root = base.join("project");
    fs::create_dir_all(&root).unwrap();
    let runtime = runtime_paths_under(&base.join("loom-home"), &root).unwrap();
    let dir = SessionDir::new(runtime.sessions.join(SESSION_ID));
    LaunchSpec {
        id: SESSION_ID.into(),
        kind: SessionKind::Authoring,
        socket: "loom-plan-contract".into(),
        tmux_session: "plan-authoring".into(),
        guidance: LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message: "OPENING".into(),
            model: "opus".into(),
            effort: "medium".into(),
            display_name: DISPLAY_NAME.into(),
            opening_on_argv: false,
        },
        dir,
        extra_env: Vec::new(),
        opening_argv: None,
        project_root: root,
        runtime,
        loom_bin: base.join("loom"),
        claude_bin: base.join("claude"),
        home: base.join("home"),
    }
}

/// An Authoring session's settings, over a project laid out as the sandbox tests lay theirs out.
fn authoring_settings(base: &Path) -> Value {
    let root = base.join("repo");
    for dir in [".git", "loom", ".claude", ".loom/work", "doc/plans"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(root.join("doc/plans/PLAN-a.md"), "# Fixture\n").unwrap();
    let home = base.join("settings-home");
    fs::create_dir_all(home.join(".loom/plan")).unwrap();
    let runtime = runtime_paths_under(&base.join("loomhome"), &root).unwrap();
    ensure_runtime_grants(&runtime).unwrap();
    let session = SessionDir::new(runtime.sessions.join("session-a"));
    fs::create_dir_all(session.done()).unwrap();
    let inputs = SandboxInputs {
        project_root: &root,
        runtime: &runtime,
        session_dir: &session,
        kind: &SessionKind::Authoring,
        loom_bin: Path::new("/usr/bin/loom"),
        home: &home,
    };
    build_settings(&inputs, &LockedSet::scan(&root).unwrap(), &[]).unwrap()
}

fn words(argv: &[OsString]) -> Vec<String> {
    argv.iter()
        .map(|w| w.to_string_lossy().into_owned())
        .collect()
}

fn after<'a>(words: &'a [String], flag: &str) -> Option<&'a str> {
    let at = words.iter().position(|word| word == flag)?;
    words.get(at + 1).map(String::as_str)
}

#[test]
fn the_side_argv_drops_the_session_identity_and_hooks() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let spec = launch_spec(&base);
    let (settings, sibling) = (spec.dir.settings(), base.join("side-settings.json"));
    let session = words(&claude_argv(&spec, &settings));
    assert_eq!(after(&session, "--session-id"), Some(SESSION_ID));

    let side = words(&side_argv(&claude_argv(&spec, &settings), &sibling));
    let brief = spec.guidance.brief_path.to_string_lossy().into_owned();
    for gone in ["--session-id", SESSION_ID, "--name", DISPLAY_NAME] {
        assert!(!side.iter().any(|w| w == gone), "{gone} kept: {side:?}");
    }
    for gone in ["--append-system-prompt-file", brief.as_str()] {
        assert!(!side.iter().any(|w| w == gone), "{gone} kept: {side:?}");
    }
    assert_eq!(side.iter().filter(|w| *w == "--settings").count(), 1);
    assert_eq!(after(&side, "--settings"), sibling.to_str());
    let plugins = spec.guidance.plugin_dir.to_str();
    assert_eq!(after(&side, "--plugin-dir"), plugins, "{side:?}");
    assert!(side.ends_with(&STREAM_TAIL.map(String::from)), "{side:?}");

    let session_settings = authoring_settings(&base);
    let hookless = side_settings(&session_settings);
    assert!(hookless.get("hooks").is_none(), "{hookless:#}");
    assert_eq!(hookless["disableAllHooks"], json!(true));
    assert_eq!(hookless["sandbox"], session_settings["sandbox"]);
    assert_eq!(hookless["permissions"], session_settings["permissions"]);
    let disabled = json!({"loom-ui@skills-dir": false});
    assert_eq!(hookless.get("enabledPlugins"), Some(&disabled));
}

/// A fake `claude` that copies its stdin to `stdin_log` and never answers.
fn silent_claude(dir: &Path, stdin_log: &Path) -> PathBuf {
    let script = dir.join("claude.sh");
    let body = format!(
        "#!/bin/sh\nexec 3<&0\ncat <&3 >'{}' &\nexec sleep 30\n",
        stdin_log.display()
    );
    fs::write(&script, body).unwrap();
    script
}

fn logged(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn alive(pid: u32) -> bool {
    let pid = Pid::from_raw(i32::try_from(pid).unwrap());
    kill(pid, None) != Err(Errno::ESRCH)
}

/// A spec that runs `silent_claude` through `/bin/sh`, with its settings at `<base>/side.json`.
fn silent_spec(base: &Path, stdin_log: &Path) -> SideSpec {
    SideSpec {
        program: PathBuf::from("/bin/sh"),
        args: vec![silent_claude(base, stdin_log).into_os_string()],
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        cwd: base.to_path_buf(),
        settings_path: base.join("side.json"),
        settings: json!({"disableAllHooks": true}),
    }
}

/// The fake's stdin was exactly one line, an `initialize` control request.
fn assert_one_initialize_line(stdin_log: &Path) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while logged(stdin_log).is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let sent = logged(stdin_log);
    let lines: Vec<&str> = sent.lines().collect();
    assert_eq!(lines.len(), 1, "stdin was not one line: {sent:?}");
    let request: Value = serde_json::from_str(lines[0]).expect("a JSON line");
    assert_eq!(request["type"], "control_request", "{request}");
    assert_eq!(request["request"]["subtype"], "initialize", "{request}");
}

#[test]
fn a_side_process_that_never_answers_is_reaped() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let stdin_log = base.join("stdin.log");
    let spec = silent_spec(&base, &stdin_log);
    let settings_path = spec.settings_path.clone();
    let (tx, rx) = mpsc::channel::<Arc<CommandTable>>();
    let on_table = Box::new(move |table: Arc<CommandTable>| {
        let _ = tx.send(table);
    });
    let side = SideProcess::start(spec, Duration::from_millis(300), on_table).unwrap();
    let pid = side.pid();
    assert!(alive(pid), "the fake claude is not running");

    let table = rx.recv_timeout(Duration::from_secs(5)).expect("no table");
    let unavailable = matches!(table.state(), CatalogueState::Unavailable { .. });
    assert!(
        unavailable,
        "a silent side process's table is not Unavailable"
    );
    let foreign: Vec<&CommandEntry> = table
        .entries()
        .iter()
        .filter(|entry| entry.source != CommandSource::BuiltIn)
        .collect();
    assert!(foreign.is_empty(), "non-native rows: {foreign:?}");
    assert!(!alive(pid), "the side process is still running or unreaped");
    assert!(!settings_path.exists(), "the side settings file is left");
    assert_one_initialize_line(&stdin_log);
    side.stop();
}

fn entry<'a>(table: &'a CommandTable, name: &str) -> &'a CommandEntry {
    let found = table.entries().iter().find(|entry| entry.name == name);
    found.unwrap_or_else(|| panic!("no {name} entry"))
}

fn assert_terminal(entry: &CommandEntry, reason: &str) {
    assert_eq!(entry.class, CommandClass::TerminalOnly, "{entry:?}");
    assert_eq!(entry.reason.as_deref(), Some(reason), "{entry:?}");
}

fn assert_runs(entry: &CommandEntry, source: CommandSource, description: &str) {
    assert_eq!((&entry.source, &entry.class), (&source, &CommandClass::Run));
    assert_eq!(entry.reason, None, "{entry:?}");
    assert!(entry.description.starts_with(description), "{entry:?}");
    for marker in ["(loom-plan)", "(user)", "(claude.ai sync)"] {
        assert!(!entry.description.contains(marker), "{marker}: {entry:?}");
    }
}

/// Every `builtin: true` row is native or bundled, and the 19 bundled skills stay bundled.
fn assert_builtin_rows_native_or_bundled(rows: &[Value], table: &CommandTable) {
    let builtin = rows.iter().filter(|row| row["builtin"] == json!(true));
    for row in builtin {
        let source = &entry(table, row["name"].as_str().unwrap()).source;
        let native_or_bundled = matches!(source, CommandSource::BuiltIn | CommandSource::Bundled);
        assert!(native_or_bundled, "{row}: {source:?}");
    }
    let bundled = table.entries().iter();
    let bundled = bundled.filter(|entry| entry.source == CommandSource::Bundled);
    assert_eq!(
        bundled.count(),
        19,
        "the probe answer holds 19 bundled skills"
    );
}

/// The native rows the planner refuses or handles, found by name or alias.
fn assert_native_classes(table: &CommandTable) {
    let clear = entry(table, "clear");
    assert_terminal(clear, "Changes the session outside the planner");
    let reset = clear.aliases.iter().any(|alias| alias == "reset");
    assert!(reset, "{clear:?}");
    let by_alias = table.lookup("reset").map(|e| e.name.as_str());
    assert_eq!(by_alias, Some("clear"));
    let usage = table.lookup("cost").expect("cost finds usage");
    assert_eq!(usage.name, "usage");
    assert_terminal(usage, "Opens a terminal dialog");
    for name in ["context", "compact"] {
        let class = &entry(table, name).class;
        assert_eq!(class, &CommandClass::RunWithHandling, "{name}");
    }
}

#[test]
fn every_builtin_row_is_native_or_bundled() {
    let rows = initialize_rows();
    let table = CommandTable::from_initialize(&rows, 1);
    let ready = matches!(table.state(), CatalogueState::Ready { generation: 1 });
    assert!(
        ready,
        "from_initialize(rows, 1) is not Ready at generation 1"
    );
    assert_builtin_rows_native_or_bundled(&rows, &table);
    assert_native_classes(&table);

    let simplify = entry(&table, "simplify");
    assert_runs(simplify, CommandSource::Bundled, "Review the changed code");
    let args = entry(&table, "loom-plan:args");
    assert_runs(args, CommandSource::Plugin, "Echo the arguments back");
    let personal = entry(&table, "probe-personal");
    assert_runs(personal, CommandSource::Personal, "Probe skill");
    let docs = entry(&table, "docs");
    assert_runs(docs, CommandSource::Account, "docs (editable");
}
