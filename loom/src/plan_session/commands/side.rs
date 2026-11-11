//! The side process: the session's own claude command line in headless stream-json mode, with
//! a sibling settings file that runs no hooks, asked once for `initialize` and never given a
//! user message. Its answer and each later `commands_changed` line become the session's table.

use std::ffi::OsString;
use std::io::{PipeWriter, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::list::CommandTable;
use super::reader::Reader;
use crate::fs::safe_fs::{
    safe_open_dirfd, safe_remove_in_workdir, safe_write_with_mode_in_workdir,
};
use crate::process::{spawn_retrying_text_busy, OwnedProcess};

/// How long the side process has to answer `initialize`.
pub const STARTUP_DEADLINE: Duration = Duration::from_secs(20);
/// The sibling settings file is the host's, readable by the user alone.
const SETTINGS_MODE: libc::mode_t = 0o600;
/// Session flags the side process drops, each with its value: the session's identity, display
/// name and brief belong to the live session alone.
const DROPPED: [&str; 3] = ["--session-id", "--name", "--append-system-prompt-file"];
/// The other flags of `claude_argv` that take a value, kept verbatim with it.
const VALUED: [&str; 5] = [
    "--setting-sources",
    "--model",
    "--effort",
    "--permission-mode",
    "--plugin-dir",
];
const HEADLESS: [&str; 7] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--no-session-persistence",
];

/// Receives each table the side process yields, on the `side-commands` thread.
pub type OnTable = Box<dyn FnMut(Arc<CommandTable>) + Send>;

/// What a side process runs: `program` with `args`, exactly `env`, in `cwd`, with `settings`
/// written to `settings_path` first.
#[derive(Debug, Clone)]
pub struct SideSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
    pub settings_path: PathBuf,
    pub settings: Value,
}

/// `session_argv` (a `claude_argv`) without the session's identity, name and brief, with
/// `--settings` pointed at `side_settings`, then the headless stream-json flags.
pub fn side_argv(session_argv: &[OsString], side_settings: &Path) -> Vec<OsString> {
    let mut words = session_argv.iter();
    let mut out: Vec<OsString> = words.next().cloned().into_iter().collect();
    while let Some(word) = words.next() {
        let flag = word.to_str().unwrap_or_default();
        if DROPPED.contains(&flag) {
            words.next();
            continue;
        }
        out.push(word.clone());
        if flag == "--settings" {
            words.next();
            out.push(side_settings.as_os_str().to_owned());
        } else if VALUED.contains(&flag) {
            out.extend(words.next().cloned());
        }
    }
    out.extend(HEADLESS.iter().map(OsString::from));
    out
}

/// The session's settings without `hooks` and with `"disableAllHooks": true`; the sandbox,
/// `permissions`, `enabledPlugins` and every other key are kept.
pub fn side_settings(session: &Value) -> Value {
    let mut settings = match session {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    settings.remove("hooks");
    settings.insert("disableAllHooks".to_owned(), Value::Bool(true));
    Value::Object(settings)
}

/// What the side process and its reader thread share: the process and its settings file.
pub(super) struct Shared {
    process: Mutex<OwnedProcess>,
    settings_dir: OwnedFd,
    settings_name: PathBuf,
    settings_left: AtomicBool,
    /// Set by `stop` and drop: no table is handed on after it.
    pub(super) stopped: AtomicBool,
}

impl Shared {
    /// Kill the process group, reap the process and delete the settings file; idempotent.
    pub(super) fn cleanup(&self) {
        let mut process = self.process.lock().unwrap_or_else(PoisonError::into_inner);
        process.retire();
        if self.settings_left.swap(false, Ordering::SeqCst) {
            let dir = self.settings_dir.as_raw_fd();
            // Best effort: the file is ours and 0600; a failure leaves nothing to retry.
            let _ = safe_remove_in_workdir(dir, &self.settings_name);
        }
    }
}

/// A running side process. `stop` and drop kill and reap it, delete its settings file and end
/// its reader thread.
pub struct SideProcess {
    pid: u32,
    shared: Arc<Shared>,
    stdin: Option<ChildStdin>,
    wake: Option<PipeWriter>,
    reader: Option<JoinHandle<()>>,
}

impl SideProcess {
    /// Write the settings file, spawn the process, send it one `initialize` line and read its
    /// stdout on the `side-commands` thread, which calls `on_table` with the answer's table
    /// (generation 1), each later `commands_changed` list (generation + 1), or once with an
    /// `Unavailable` table when no answer comes within `deadline` or the output breaks a bound.
    pub fn start(spec: SideSpec, deadline: Duration, on_table: OnTable) -> Result<Self> {
        let deadline = Instant::now() + deadline;
        let (settings_dir, settings_name) = write_settings(&spec.settings_path, &spec.settings)?;
        let child = match spawn_retrying_text_busy(&mut command(&spec)) {
            Ok(child) => child,
            Err(error) => {
                let _ = safe_remove_in_workdir(settings_dir.as_raw_fd(), &settings_name);
                let program = spec.program.display();
                return Err(error).with_context(|| format!("cannot start {program}"));
            }
        };
        let mut process = OwnedProcess::new(child);
        let (stdin, stdout) = (process.child().stdin.take(), process.child().stdout.take());
        let mut side = SideProcess {
            pid: process.child().id(),
            shared: Arc::new(Shared {
                process: Mutex::new(process),
                settings_dir,
                settings_name,
                settings_left: AtomicBool::new(true),
                stopped: AtomicBool::new(false),
            }),
            stdin,
            wake: None,
            reader: None,
        };
        // From here an error drops `side`, which kills, reaps and cleans up.
        let stdout = stdout.context("the side process has no stdout pipe")?;
        let request_id = side.send_initialize();
        let (wake_read, wake_write) = std::io::pipe().context("cannot create the wake pipe")?;
        side.wake = Some(wake_write);
        let shared = Arc::clone(&side.shared);
        let reader = Reader::new(stdout, wake_read, shared, on_table, request_id, deadline);
        let thread = thread::Builder::new().name("side-commands".into());
        let handle = thread.spawn(move || reader.run());
        side.reader = Some(handle.context("cannot start the side-commands thread")?);
        Ok(side)
    }

    /// The side process's pid.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Kill and reap the process, delete its settings file and end the reader thread.
    pub fn stop(mut self) {
        self.shutdown();
    }

    /// Write the one `initialize` control request and keep stdin open; returns its id. A
    /// failed write (the process already gone) is not reported here: the reader sees the end
    /// of stdout or the deadline and reports the table as unavailable.
    fn send_initialize(&mut self) -> String {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = format!("loom-side-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let request = json!({
            "type": "control_request",
            "request_id": id,
            "request": {"subtype": "initialize"},
        });
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = writeln!(stdin, "{request}").and_then(|()| stdin.flush());
        }
        id
    }

    fn shutdown(&mut self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        self.shared.cleanup();
        self.stdin.take();
        // Closing the wake pipe ends the reader's poll at once.
        self.wake.take();
        if let Some(reader) = self.reader.take() {
            // A callback that drops its own side process runs on the reader thread.
            if reader.thread().id() != thread::current().id() {
                let _ = reader.join();
            }
        }
    }
}

impl Drop for SideProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The spec's command in its own process group, with exactly the spec's environment (none of
/// the server's own `LOOM_*` variables), stdin and stdout piped and stderr discarded.
fn command(spec: &SideSpec) -> Command {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args).env_clear();
    command.envs(spec.env.iter().cloned());
    command.current_dir(&spec.cwd).process_group(0);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    command.stderr(Stdio::null());
    command
}

/// Write `settings` to `path` (0600, refusing a symlink); returns the parent directory's fd
/// and the file name, from which the file is deleted later.
fn write_settings(path: &Path, settings: &Value) -> Result<(OwnedFd, PathBuf)> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let parent = parent.with_context(|| format!("{} has no parent", path.display()))?;
    let name = path.file_name();
    let name = PathBuf::from(name.with_context(|| format!("{} has no name", path.display()))?);
    let dir = safe_open_dirfd(parent)
        .with_context(|| format!("cannot open the directory {}", parent.display()))?;
    let body = serde_json::to_vec_pretty(settings).context("cannot serialise side settings")?;
    safe_write_with_mode_in_workdir(dir.as_raw_fd(), &name, &body, SETTINGS_MODE)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok((dir, name))
}
