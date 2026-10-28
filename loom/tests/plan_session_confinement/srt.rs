//! srt for the planning-session confinement test: whether this host can run it, a session's
//! `sandbox.filesystem` block translated into srt settings, and probes run under them.
//!
//! The translation reads the block the way Claude Code does: `~/` is the session's home, a
//! relative entry is relative to the working directory, the working directory itself is
//! writable, and the network is closed. Unlike the stage capsule translation
//! (`orchestrator/terminal/native/tests_confinement_srt.rs`, an in-crate module this test
//! cannot import) it also passes `allowRead` through: the runtime carve-outs under the
//! planning-root deny exist only there, and there is no fallback when srt cannot express them.
//!
//! A probe counts only when its shell provably ran under srt: the shell prints `ALIVE` first
//! and the command's own exit code last. srt's exit status cannot stand in for either: an srt
//! that crashes before starting the shell exits non-zero and leaves every file as it was, which
//! reads exactly like a refused operation.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

use loom::process::sandbox_probe::skip_unless;
use serde_json::{json, Value};
use shell_escape::escape;
use tempfile::TempDir;

use super::fixture::Fixture;

/// The line a sandboxed shell prints first: proof that srt started it.
const ALIVE: &str = "LOOM_PROBE_ALIVE";
/// The prefix of the line carrying the probed command's own exit code.
const RC_PREFIX: &str = "LOOM_PROBE_RC=";
/// How much of srt's stderr a failure message quotes.
const STDERR_LINES: usize = 40;

/// Whether `test_name` must skip because this host cannot run srt. A skip prints its reason;
/// `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns it into a panic so the operator run cannot pass by
/// skipping. In the criteria runner and in a session the nested sandbox cannot start, so the
/// test skips there.
pub fn skip(test_name: &str) -> bool {
    static REASON: OnceLock<Option<String>> = OnceLock::new();
    let reason = REASON.get_or_init(probe_srt).as_deref();
    skip_unless(reason.is_none(), test_name, reason.unwrap_or_default())
}

/// Why this host cannot run srt, or `None` when it can.
fn probe_srt() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("the confinement test runs on Linux only".to_string());
    }
    if let Some(tool) = ["bwrap", "socat", "srt"]
        .into_iter()
        .find(|tool| !on_path(tool))
    {
        return Some(format!("`{tool}` is not on PATH"));
    }
    let dir = TempDir::new().expect("create a temp dir for the srt probe");
    let settings = dir.path().join("srt.json");
    let document = srt_document(vec![], vec![], vec![], vec![]);
    std::fs::write(&settings, document.to_string()).unwrap();
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

/// `output`'s exit status, stdout and the tail of stderr, for a failure message. A crashed srt
/// prints its exception above Node's closing version line, so the last line alone hides it.
fn diagnostics(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    let cut = lines.len().saturating_sub(STDERR_LINES);
    format!(
        "{}\nstdout:\n{}\nstderr ({cut} earlier lines cut):\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim_end(),
        lines[cut..].join("\n")
    )
}

fn srt_document(
    deny_read: Vec<String>,
    allow_read: Vec<String>,
    allow_write: Vec<String>,
    deny_write: Vec<String>,
) -> Value {
    json!({
        "filesystem": {
            "denyRead": deny_read,
            "allowRead": allow_read,
            "allowWrite": allow_write,
            "denyWrite": deny_write,
        },
        "network": {"allowedDomains": [], "deniedDomains": []},
    })
}

/// The settings' `sandbox.filesystem` block as srt settings for a session running in `cwd`
/// with `home` as its home directory.
fn srt_settings(settings: &Value, cwd: &Path, home: &Path) -> Value {
    let list = |key: &str| -> Vec<String> {
        let entries = settings["sandbox"]["filesystem"][key].as_array();
        entries
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|entry| srt_path(entry, cwd, home))
            .collect()
    };
    let mut allow_write = vec![cwd.display().to_string()];
    allow_write.extend(list("allowWrite"));
    srt_document(
        list("denyRead"),
        list("allowRead"),
        allow_write,
        list("denyWrite"),
    )
}

/// One settings path as the absolute path srt takes: `~/` under the session's home, `//`
/// collapsed, a relative entry under the working directory, and a directory's trailing `/**`
/// cut. `None` for any other glob, which srt would expand by walking the tree; no probe here
/// depends on one.
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

/// `path` as one shell word.
pub fn quote(path: impl AsRef<Path>) -> String {
    escape(Cow::Owned(path.as_ref().display().to_string())).into_owned()
}

/// One probe: the command's own exit code, `None` when the shell died before printing it.
pub struct Probe {
    pub rc: Option<i32>,
    output: Output,
}

impl Probe {
    pub fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    /// srt's output, for a failure message.
    pub fn diag(&self) -> String {
        diagnostics(&self.output)
    }
}

/// The authoring session's settings, translated for srt.
pub struct Confined {
    /// The srt settings file.
    settings: PathBuf,
    /// The session's working directory: the project root.
    cwd: PathBuf,
    home: PathBuf,
    loom_home: PathBuf,
}

impl Confined {
    pub fn new(fixture: &Fixture) -> Self {
        let settings = fixture.base.join("srt.json");
        let document = srt_settings(&fixture.settings, &fixture.root, &fixture.home);
        std::fs::write(&settings, document.to_string()).unwrap();
        Confined {
            settings,
            cwd: fixture.root.clone(),
            home: fixture.home.clone(),
            loom_home: fixture.loom_home.clone(),
        }
    }

    /// Run `command` under srt from the project root, with `envs` added to an environment
    /// holding no `LOOM_*` variable but `LOOM_HOME`, and fail, quoting srt's output, when the
    /// shell did not print `ALIVE` first: srt never started it, so nothing `command` was
    /// meant to show happened.
    pub fn run(&self, command: &str, envs: &[(&str, &str)]) -> Probe {
        let mut srt = Command::new("srt");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LOOM_") {
                srt.env_remove(key);
            }
        }
        srt.env("HOME", &self.home)
            .env("LOOM_HOME", &self.loom_home)
            .envs(envs.iter().copied());
        let script = format!("echo {ALIVE}; {command}; echo {RC_PREFIX}$?");
        let output = srt
            .arg("--settings")
            .arg(&self.settings)
            .arg("-c")
            .arg(script)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .output()
            .expect("run srt");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let alive = stdout.lines().any(|line| line.trim() == ALIVE);
        assert!(
            alive,
            "srt did not start under the authoring settings: {}",
            diagnostics(&output)
        );
        let rc = stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix(RC_PREFIX))
            .and_then(|rc| rc.parse().ok());
        Probe { rc, output }
    }
}
