//! srt for the confinement e2e: whether this host can run it, a capsule's
//! `sandbox.filesystem` block translated into srt settings, and write probes
//! run under them.
//!
//! The translation reads the block the way Claude Code does: `~/` is the
//! session's home, a relative entry is relative to the working directory,
//! the working directory itself is writable, and the network is closed.
//!
//! A probe counts only when its shell provably ran under srt: the shell
//! prints `ALIVE` first and, for a write, the write's own exit code after
//! it. srt's exit status cannot stand in for either: an srt that crashes
//! before starting the shell exits non-zero and leaves every file as it
//! was, which reads exactly like a refused write.

use crate::process::sandbox_probe::skip_unless;
use serde_json::{json, Value};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use tempfile::TempDir;

/// What every write probe writes.
const PROBE: &str = "confinement-e2e-probe";
/// The line a sandboxed shell prints first: proof that srt started it.
const ALIVE: &str = "LOOM_PROBE_ALIVE";
/// The prefix of the line carrying a probe write's own exit code.
const RC_PREFIX: &str = "LOOM_PROBE_RC=";
/// How much of srt's stderr a failure message quotes.
const STDERR_LINES: usize = 40;

/// Whether `test_name` must skip because this host cannot run srt: it is not
/// Linux, `bwrap`, `socat` or `srt` is missing from PATH, or a trivial
/// `srt -c true` fails, as it does where a nested sandbox refuses the
/// namespaces or the sockets srt needs.
pub(super) fn skip(test_name: &str) -> bool {
    let reason = srt_unavailable();
    skip_unless(reason.is_none(), test_name, reason.unwrap_or_default())
}

/// Why this host cannot run srt, or `None` when it can. Memoized: it is a
/// fact about the host, and every test asks.
fn srt_unavailable() -> Option<&'static str> {
    static REASON: OnceLock<Option<String>> = OnceLock::new();
    REASON.get_or_init(probe_srt).as_deref()
}

fn probe_srt() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("the confinement e2e runs on Linux only".to_string());
    }
    if let Some(tool) = ["bwrap", "socat", "srt"]
        .into_iter()
        .find(|tool| !on_path(tool))
    {
        return Some(format!("`{tool}` is not on PATH"));
    }
    let dir = TempDir::new().expect("create a temp dir for the srt probe");
    let settings = dir.path().join("srt.json");
    std::fs::write(&settings, srt_document(vec![], vec![], vec![]).to_string()).unwrap();
    let output = Command::new("srt")
        .arg("--settings")
        .arg(&settings)
        .args(["-c", "true"])
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(format!(
            "`srt -c true` fails here, so this host cannot nest srt's sandbox: {}",
            diagnostics(&output)
        )),
        Err(error) => Some(format!("cannot run srt: {error}")),
    }
}

fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
}

/// `output`'s exit status, stdout, and stderr (whole, or its first
/// `STDERR_LINES` lines), for a failure message. A crashed srt prints its
/// exception above Node's closing version line, so the last line alone
/// hides it.
pub(super) fn diagnostics(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    let cut = lines.len().saturating_sub(STDERR_LINES);
    let more = if cut > 0 {
        format!("\n[{cut} more lines]")
    } else {
        String::new()
    };
    format!(
        "{}\nstdout:\n{}\nstderr:\n{}{more}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim_end(),
        lines[..lines.len() - cut].join("\n")
    )
}

/// Whether the sandboxed shell printed `ALIVE`, alone on its line.
fn started(output: &Output) -> bool {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim() == ALIVE)
}

/// The probe write's exit code from its `RC_PREFIX` line, `None` when the
/// shell died before printing one.
fn write_rc(output: &Output) -> Option<i32> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let rc = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix(RC_PREFIX));
    rc.and_then(|rc| rc.parse().ok())
}

/// Write `capsule`'s sandbox block to `path` as srt settings, for a session
/// running in `cwd` with `home` as its home directory.
pub(super) fn write_settings(path: &Path, capsule: &Value, cwd: &Path, home: &Path) {
    std::fs::write(path, srt_settings(capsule, cwd, home).to_string()).unwrap();
}

fn srt_settings(capsule: &Value, cwd: &Path, home: &Path) -> Value {
    let list = |key: &str| -> Vec<String> {
        let entries = capsule["sandbox"]["filesystem"][key].as_array();
        entries
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|entry| srt_path(entry, cwd, home))
            .collect()
    };
    let mut allow_write = vec![cwd.display().to_string()];
    allow_write.extend(list("allowWrite"));
    srt_document(list("denyRead"), allow_write, list("denyWrite"))
}

fn srt_document(
    deny_read: Vec<String>,
    allow_write: Vec<String>,
    deny_write: Vec<String>,
) -> Value {
    json!({
        "filesystem": {
            "denyRead": deny_read,
            "allowWrite": allow_write,
            "denyWrite": deny_write,
        },
        "network": {"allowedDomains": [], "deniedDomains": []},
    })
}

/// One capsule path as the absolute path srt takes: `~/` under the session's
/// home, `//` collapsed, a relative entry under the working directory, and a
/// directory's trailing `/**` cut. `None` for any other glob, which srt would
/// expand by walking the tree; no probe here depends on one.
fn srt_path(entry: &str, cwd: &Path, home: &Path) -> Option<String> {
    let entry = entry.strip_suffix("/**").unwrap_or(entry);
    if entry.contains(['*', '?', '[', '{']) {
        return None;
    }
    let path = match (entry.strip_prefix("~/"), entry.strip_prefix("//")) {
        (Some(rest), _) => home.join(rest),
        (None, Some(rest)) => Path::new("/").join(rest),
        (None, None) => cwd.join(entry),
    };
    Some(path.display().to_string())
}

/// One launched session's capsule, translated for srt.
pub(super) struct Confined {
    pub(super) label: String,
    /// The srt settings file `write_settings` wrote.
    pub(super) settings: PathBuf,
    /// The session's working directory.
    pub(super) cwd: PathBuf,
    /// The session's own scratch directory.
    pub(super) scratch: PathBuf,
}

/// One write probe, from a shell srt is known to have started.
pub(super) struct Probe {
    /// The write's own exit code; `None` when the shell died before
    /// printing it.
    pub(super) rc: Option<i32>,
    /// srt's output, for a failure message.
    pub(super) output: Output,
}

impl Confined {
    /// Run `command` under srt with this capsule, from the session's
    /// working directory.
    fn run(&self, command: &str) -> Output {
        Command::new("srt")
            .arg("--settings")
            .arg(&self.settings)
            .arg("-c")
            .arg(command)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .output()
            .expect("run srt")
    }

    /// Run `command` under this capsule after an `echo` of `ALIVE`, and
    /// fail, quoting srt's output, when that line is missing: srt never
    /// started the shell, so nothing `command` was meant to show happened.
    pub(super) fn run_alive(&self, command: &str) -> Output {
        let output = self.run(&format!("echo {ALIVE}; {command}"));
        assert!(
            started(&output),
            "{}: srt did not start under this capsule: {}",
            self.label,
            diagnostics(&output)
        );
        output
    }

    /// Try to write the probe to `path` under this capsule.
    pub(super) fn write(&self, path: &Path) -> Probe {
        let target = escape(Cow::Owned(path.display().to_string()));
        let output = self.run_alive(&format!("printf {PROBE} > {target}; echo {RC_PREFIX}$?"));
        Probe {
            rc: write_rc(&output),
            output,
        }
    }

    /// Each of `denied` whose write was not proven refused: that takes a
    /// non-zero exit code from the write itself and the path left as it
    /// was, or still absent.
    pub(super) fn refusals_missed(&self, denied: &[PathBuf]) -> Vec<String> {
        let mut missed = Vec::new();
        for path in denied {
            let before = std::fs::read(path).ok();
            let probe = self.write(path);
            let changed = std::fs::read(path).ok() != before;
            let refused = probe.rc.is_some_and(|rc| rc != 0) && !changed;
            if !refused {
                missed.push(format!(
                    "{}: a write to {} was not refused (write exit code {:?}, changed: \
                     {changed}): {}",
                    self.label,
                    path.display(),
                    probe.rc,
                    diagnostics(&probe.output)
                ));
            }
        }
        missed
    }

    /// Each of `allowed` whose write was not proven to land: that takes
    /// exit code 0 from the write itself and the probe in the file.
    pub(super) fn writes_missed(&self, allowed: &[PathBuf]) -> Vec<String> {
        let mut missed = Vec::new();
        for path in allowed {
            let probe = self.write(path);
            let landed = std::fs::read(path).is_ok_and(|bytes| bytes == PROBE.as_bytes());
            let wrote = probe.rc == Some(0) && landed;
            if !wrote {
                missed.push(format!(
                    "{}: a write to {} must succeed (write exit code {:?}, landed: \
                     {landed}): {}",
                    self.label,
                    path.display(),
                    probe.rc,
                    diagnostics(&probe.output)
                ));
            }
        }
        missed
    }
}
