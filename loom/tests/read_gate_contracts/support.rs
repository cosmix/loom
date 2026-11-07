//! Builders for the read-gate contracts: the real hook scripts installed from `LOOM_HOOKS`, a
//! stage environment for each run, and the assertions a hook decision is judged by. A contract
//! never self-skips, so nothing here probes the sandbox.

use crate::helpers;
use loom::fs::permissions::constants::LOOM_HOOKS;
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const OVER_THE_LINE_LIMIT: usize = 500;
const REPEAT_ALLOWED: &str = "Repeating this read is allowed";
const FIRST_READ_TAG: &str = "[read-discipline/first-read]";
const NO_OUTLINE: &str = "no outline is available";
/// A `loom` whose every invocation prints one outline row the hook's row regex keeps.
const OUTLINE_LOOM: &str = "#!/usr/bin/env bash\nprintf '%s\\n' '-> Outline: stub' \
    '  L1-L500\tfunction\tbody' 'coverage: full'\n";

/// What a hook run can see of the session above it.
#[derive(Clone, Copy)]
pub enum Liveness {
    /// `LOOM_STAGE_ID` set, `LOOM_MAIN_AGENT_PID` naming this test process, a live ancestor.
    Live,
    /// `LOOM_STAGE_ID` set, `LOOM_MAIN_AGENT_PID` naming a process that has exited.
    DeadMainAgent,
    /// Neither variable set.
    NoStage,
}

/// One hook's exit code and streams.
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    fn from_output(output: Output) -> Self {
        Run {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    fn describe(&self) -> String {
        format!(
            "exit {}\nstdout: {}\nstderr: {}",
            self.code, self.stdout, self.stderr
        )
    }

    /// Exit 2, the first non-empty stderr line opening with the rule tag, and
    /// the sentence that tells the agent a repeat will pass.
    #[track_caller]
    pub fn assert_denied(&self, what: &str) {
        assert_eq!(self.code, 2, "{what}: {}", self.describe());
        let first = self.stderr.lines().find(|line| !line.trim().is_empty());
        assert!(
            self.stderr.contains(REPEAT_ALLOWED)
                && first.is_some_and(|line| line.starts_with(FIRST_READ_TAG)),
            "{what}: {}",
            self.describe()
        );
    }

    #[track_caller]
    pub fn assert_allowed(&self, what: &str) {
        assert_eq!(self.code, 0, "{what}: {}", self.describe());
    }

    /// Exit 0 and nothing on stdout: no denial and no warning.
    #[track_caller]
    pub fn assert_silently_allowed(&self, what: &str) {
        self.assert_allowed(what);
        assert!(self.stdout.trim().is_empty(), "{what}: {}", self.describe());
    }

    /// Exit 0 with the queued warning naming `path`.
    #[track_caller]
    pub fn assert_warned(&self, path: &Path, what: &str) {
        self.warning(path, what);
    }

    /// Exit 0 with the warning naming `path` and the guidance for a file with no outline.
    #[track_caller]
    pub fn assert_unoutlined_warned(&self, path: &Path, what: &str) {
        let context = self.warning(path, what);
        assert!(context.contains(NO_OUTLINE), "{what}: {}", self.describe());
    }

    #[track_caller]
    fn warning(&self, path: &Path, what: &str) -> String {
        self.assert_allowed(what);
        let parsed: Value = serde_json::from_str(self.stdout.trim())
            .unwrap_or_else(|error| panic!("{what}: {error}: {}", self.describe()));
        let context = parsed["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap_or_default();
        assert!(
            context.contains("LOOM_HOOK_WARN") && context.contains(&*path.to_string_lossy()),
            "{what}: {}",
            self.describe()
        );
        context.to_string()
    }
}

/// The hook scripts and a work directory, with the identity the next run reports. Fields are
/// plain so a contract changes one between runs.
pub struct Gate {
    hooks: TempDir,
    work: TempDir,
    pub session: &'static str,
    pub liveness: Liveness,
    pub cwd: Option<PathBuf>,
    pub loom_bin: Option<PathBuf>,
}

impl Gate {
    /// A live stage with no `config.toml`, so `deny_enabled` is unset, and a missing `LOOM_BIN`,
    /// so no file has an outline and rule 1 only warns.
    pub fn live() -> Self {
        let hooks = TempDir::new().expect("hook dir");
        for (name, body) in LOOM_HOOKS {
            let path = hooks.path().join(name);
            fs::write(&path, body).expect("write hook");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod hook");
        }
        let work = TempDir::new().expect("work dir");
        let home = work.path().join("loom-home");
        fs::create_dir_all(&home).expect("scratch LOOM_HOME");
        fs::write(home.join("config.toml"), "[update]\ncheck = false\n").expect("user config");
        Gate {
            hooks,
            work,
            session: "sess-a",
            liveness: Liveness::Live,
            cwd: None,
            loom_bin: None,
        }
    }

    /// [`Gate::live`] with a `LOOM_BIN` that outlines every file, so a first whole read is denied.
    pub fn outlined() -> Self {
        let mut gate = Gate::live();
        let bin = gate.work_dir().join("outline-loom");
        fs::write(&bin, OUTLINE_LOOM).expect("write outline loom");
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).expect("chmod outline loom");
        gate.loom_bin = Some(bin);
        gate
    }

    pub fn work_dir(&self) -> &Path {
        self.work.path()
    }

    /// The stage block ledger rows `loom_record_block` wrote for this gate's stage.
    pub fn blocks(&self) -> Vec<Value> {
        let ledger = self.work_dir().join("heartbeat/stage-1.blocks.jsonl");
        fs::read_to_string(ledger)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("ledger row is JSON"))
            .collect()
    }

    pub fn write_config(&self, body: &str) {
        fs::write(self.work_dir().join("config.toml"), body).expect("write config.toml");
    }

    /// A whole Read of `path` through read-guard.sh.
    pub fn read(&self, agent: &str, path: &Path) -> Run {
        self.run(
            "read-guard.sh",
            json!({
                "tool_name": "Read",
                "tool_input": {"file_path": path},
                "agent_id": agent,
                "session_id": self.session,
                "cwd": self.current_dir(),
            }),
        )
    }

    /// A Bash `command` through poll-guard.sh.
    pub fn bash(&self, agent: &str, command: &str) -> Run {
        self.run(
            "poll-guard.sh",
            json!({
                "tool_name": "Bash",
                "tool_input": {"command": command},
                "agent_id": agent,
                "session_id": self.session,
                "cwd": self.current_dir(),
            }),
        )
    }

    fn current_dir(&self) -> &Path {
        self.cwd.as_deref().unwrap_or(self.work_dir())
    }

    fn run(&self, hook: &str, payload: Value) -> Run {
        let missing_loom = self.work_dir().join("no-such-loom");
        let mut command = Command::new("bash");
        command.arg(self.hooks.path().join(hook));
        helpers::clear_relay_env(&mut command)
            .current_dir(self.current_dir())
            .env("LOOM_WORK_DIR", self.work_dir())
            .env("LOOM_SESSION_ID", self.session)
            .env("LOOM_HOME", self.work_dir().join("loom-home"))
            .env("TMPDIR", self.work_dir());
        match &self.loom_bin {
            Some(bin) => command.env("LOOM_BIN", bin),
            None => command.env("LOOM_BIN", missing_loom),
        };
        match self.liveness {
            Liveness::Live => {
                command
                    .env("LOOM_STAGE_ID", "stage-1")
                    .env("LOOM_MAIN_AGENT_PID", std::process::id().to_string());
            }
            Liveness::DeadMainAgent => {
                command
                    .env("LOOM_STAGE_ID", "stage-1")
                    .env("LOOM_MAIN_AGENT_PID", exited_pid().to_string());
            }
            Liveness::NoStage => {}
        }
        Run::from_output(helpers::run_hook_process(
            &mut command,
            payload.to_string().as_bytes(),
        ))
    }
}

fn exited_pid() -> u32 {
    let mut child = Command::new("true").spawn().expect("spawn true");
    let pid = child.id();
    child.wait().expect("reap true");
    pid
}

/// Write a file over `READ_GUARD_LINE_LIMIT` lines at `root/relative`, creating its directories.
pub fn big_file(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("file has a parent")).expect("create parents");
    fs::write(&path, "line\n".repeat(OVER_THE_LINE_LIMIT)).expect("write big file");
    path
}

/// `loom map --outline <arg>` run from `repo`.
pub fn map_outline(repo: &Path, arg: &str) -> Run {
    let mut command = helpers::loom_cmd();
    command.args(["map", "--outline", arg]).current_dir(repo);
    Run::from_output(helpers::run_hook_process(&mut command, b""))
}

/// The lines the hook's own row regex keeps from `loom map --outline` output
/// (`_loom_outline_covered_rows` in `loom-hooks/_read_discipline.sh`).
pub fn outline_rows(stdout: &str) -> Vec<&str> {
    let row = Regex::new(r"^[[:space:]]+L[0-9]+-L[0-9]+[[:space:]]").expect("row regex");
    stdout.lines().filter(|line| row.is_match(line)).collect()
}
