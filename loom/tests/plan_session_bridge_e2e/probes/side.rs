//! The B-D1 side process: the live session's argv in headless stream-json mode with a sibling
//! settings file that runs no hooks, and a bounded child that records its stdout.

use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use serde_json::Value;

use super::procs::{kill_all, proc_of, tree, Seen};

/// Session flags B-D1 drops, each with its value.
const DROPPED: [&str; 3] = ["--session-id", "--name", "--append-system-prompt-file"];
const HEADLESS: [&str; 7] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--no-session-persistence",
];
const DROPPED_ENV: [&str; 2] = ["LOOM_PLAN_SESSION", "LOOM_PLAN_STATE"];
/// A stdout line is cut past this; reading stops past `MAX_TOTAL` (B-D1's output bounds).
const MAX_LINE: usize = 1024 * 1024;
const MAX_TOTAL: usize = 8 * 1024 * 1024;

/// The live argv without the flags B-D1 drops, `--settings` pointed at `settings`, then the
/// headless flags and `extra`.
pub fn side_argv(session: &[OsString], settings: &Path, extra: &[&str]) -> Vec<OsString> {
    let mut out = Vec::new();
    let mut words = session.iter();
    while let Some(word) = words.next() {
        let text = word.to_str().unwrap_or_default();
        if DROPPED.contains(&text) {
            words.next();
            continue;
        }
        out.push(word.clone());
        if text == "--settings" {
            words.next();
            out.push(settings.as_os_str().to_owned());
        }
    }
    out.extend(HEADLESS.iter().chain(extra).map(OsString::from));
    out
}

/// The value after `--settings` in `argv`.
pub fn settings_of(argv: &[OsString]) -> Option<PathBuf> {
    let at = argv.iter().position(|word| word == "--settings")?;
    argv.get(at + 1).map(PathBuf::from)
}

/// The session settings without `hooks`, with `"disableAllHooks": true`, and `enabledPlugins`
/// set to `plugins`, or removed when `None`.
pub fn side_settings(session: &Value, plugins: Option<&Value>) -> Value {
    let mut settings = session.clone();
    if let Some(map) = settings.as_object_mut() {
        map.remove("hooks");
        map.insert("disableAllHooks".to_owned(), Value::Bool(true));
        match plugins {
            Some(plugins) => map.insert("enabledPlugins".to_owned(), plugins.clone()),
            None => map.remove("enabledPlugins"),
        };
    }
    settings
}

fn nul_words(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split(|b| *b == 0).filter(|word| !word.is_empty())
}

pub fn live_argv(pid: u32) -> io::Result<Vec<OsString>> {
    let bytes = fs::read(format!("/proc/{pid}/cmdline"))?;
    let word = |w: &[u8]| std::ffi::OsStr::from_bytes(w).to_owned();
    Ok(nul_words(&bytes).map(word).collect())
}

/// The live environment without `LOOM_PLAN_SESSION` and `LOOM_PLAN_STATE`.
pub fn live_env(pid: u32) -> io::Result<Vec<(OsString, OsString)>> {
    let bytes = fs::read(format!("/proc/{pid}/environ"))?;
    let pairs = nul_words(&bytes).filter_map(|entry| {
        let at = entry.iter().position(|b| *b == b'=')?;
        let name = std::ffi::OsStr::from_bytes(&entry[..at]).to_owned();
        let value = std::ffi::OsStr::from_bytes(&entry[at + 1..]).to_owned();
        Some((name, value))
    });
    let kept =
        |(name, _): &(OsString, OsString)| !DROPPED_ENV.iter().any(|dropped| name == dropped);
    Ok(pairs.filter(kept).collect())
}

/// A running side process; dropping it kills its process group and reaps it.
pub struct Side {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    started: Instant,
    seen: Seen,
    stopped: Option<String>,
    /// Each stdout line with its time after the spawn.
    pub out: Vec<(Duration, String)>,
}

fn read_lines(stdout: impl io::Read, send: mpsc::Sender<String>) {
    let mut reader = BufReader::new(stdout);
    let mut total = 0;
    let mut line = String::new();
    while total < MAX_TOTAL && matches!(reader.read_line(&mut line), Ok(n) if n > 0) {
        total += line.len();
        let mut text = std::mem::take(&mut line);
        if text.len() > MAX_LINE {
            let cut = (0..=MAX_LINE).rev().find(|at| text.is_char_boundary(*at));
            text.truncate(cut.unwrap_or(0));
        }
        if send.send(text.trim_end().to_owned()).is_err() {
            return;
        }
    }
}

impl Side {
    /// Starts `argv` in its own process group with exactly `env`, in `cwd`, stderr to
    /// `stderr`; registers it with `seen` so the probe's teardown kills it too.
    pub fn spawn(
        argv: &[OsString],
        env: &[(OsString, OsString)],
        cwd: &Path,
        stderr: &Path,
        seen: Seen,
    ) -> io::Result<Side> {
        let (program, args) = argv.split_first().ok_or(io::ErrorKind::InvalidInput)?;
        let mut command = Command::new(program);
        command
            .args(args)
            .env_clear()
            .envs(env.iter().cloned())
            .current_dir(cwd);
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        command.stderr(fs::File::create(stderr)?).process_group(0);
        let mut child = command.spawn()?;
        seen.add(proc_of(child.id()));
        let stdout = child.stdout.take().ok_or(io::ErrorKind::BrokenPipe)?;
        let (send, lines) = mpsc::channel();
        let reader = thread::spawn(move || read_lines(stdout, send));
        Ok(Side {
            stdin: child.stdin.take(),
            child,
            lines,
            reader: Some(reader),
            started: Instant::now(),
            seen,
            stopped: None,
            out: Vec::new(),
        })
    }

    /// Writes one JSON line to its stdin; the pipe stays open, as a client's would.
    pub fn send(&mut self, line: &str) -> io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or(io::ErrorKind::BrokenPipe)?;
        stdin.write_all(line.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()
    }

    /// Collects stdout lines until one satisfies `done` or `limit` passes; returns that line's
    /// time after the spawn.
    pub fn until(&mut self, limit: Duration, done: impl Fn(&Value) -> bool) -> Option<Duration> {
        let deadline = Instant::now() + limit;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let at = self.started.elapsed();
                    let value = serde_json::from_str(&line).unwrap_or(Value::Null);
                    self.out.push((at, line));
                    if done(&value) {
                        return Some(at);
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    /// Kills its process group and every descendant, reaps it and joins the reader; returns
    /// how it exited. Calling it again returns the same text.
    pub fn stop(&mut self) -> String {
        if let Some(stopped) = &self.stopped {
            return stopped.clone();
        }
        let pid = self.child.id();
        let procs = tree(pid);
        self.seen.add(procs.iter().copied());
        let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
        let _ = self.child.kill();
        let status = self.child.wait();
        let left = kill_all(&procs);
        self.stdin.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        let text = format!("exit: {status:?}; descendants left running: {}", left.len());
        self.stopped = Some(text.clone());
        text
    }
}

impl Drop for Side {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn words(text: &str) -> Vec<OsString> {
        text.split(' ').map(OsString::from).collect()
    }

    #[test]
    fn side_argv_drops_the_session_flags_and_adds_the_headless_ones() {
        let session = words(
            "claude --session-id 1-2 --name loom --settings /s/settings.json --model sonnet \
             --plugin-dir /p --append-system-prompt-file /b.md",
        );
        let argv = side_argv(&session, Path::new("/s/side.json"), &["--max-turns", "0"]);
        let want = words(
            "claude --settings /s/side.json --model sonnet --plugin-dir /p -p --input-format \
             stream-json --output-format stream-json --verbose --no-session-persistence \
             --max-turns 0",
        );
        assert_eq!(argv, want);
        assert_eq!(
            settings_of(&session),
            Some(PathBuf::from("/s/settings.json"))
        );
    }

    #[test]
    fn side_settings_remove_hooks_and_set_the_plugins() {
        let session = json!({"hooks": {"Stop": []}, "sandbox": {"enabled": true}});
        let off = json!({"loom-ui@skills-dir": false});
        let want = json!({"sandbox": {"enabled": true}, "disableAllHooks": true,
                          "enabledPlugins": {"loom-ui@skills-dir": false}});
        assert_eq!(side_settings(&session, Some(&off)), want);
        let removed = side_settings(&want, None);
        assert_eq!(
            removed,
            json!({"sandbox": {"enabled": true}, "disableAllHooks": true})
        );
    }
}
