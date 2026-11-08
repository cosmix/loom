//! Fixture for the hook false-positive contracts: a copy of every hook script, a fixture
//! repository with one real git worktree, and a runner that feeds one PreToolUse payload to one
//! hook, optionally at the leaf of a fake Claude Code process chain.
//!
//! The temporary root holds no "claude": the live-session detector matches that name anywhere in
//! a process command line, so a fixture under such a root would make every process (the hook
//! included) look like a Claude ancestor. Only the chain scripts carry the name.

use crate::test_support;
use serde_json::{json, Value};
use std::fs;
use std::io::{Seek, Write};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use tempfile::TempDir;

/// Who the hook believes it is serving.
#[derive(Clone, Copy)]
pub enum Agent<'a> {
    /// The hook runs directly, with no `agent_type` and only the `LOOM_MAIN_AGENT_PID` the
    /// caller's env names.
    Plain,
    /// claude-outer (exports `LOOM_MAIN_AGENT_PID=$$`) -> claude-mid -> claude-inner -> hook,
    /// and the payload carries `agent_type` set to the given value.
    Subagent(&'a str),
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

    pub fn describe(&self) -> String {
        format!(
            "exit {}\nstdout: {}\nstderr: {}",
            self.code, self.stdout, self.stderr
        )
    }

    #[track_caller]
    pub fn assert_code(&self, code: i32, what: &str) {
        assert_eq!(self.code, code, "{what}: {}", self.describe());
    }

    /// The `hookSpecificOutput` object of the JSON decision on stdout.
    #[track_caller]
    pub fn hook_output(&self, what: &str) -> Value {
        let parsed: Value = serde_json::from_str(self.stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "{what}: stdout is no JSON decision ({error}): {}",
                self.describe()
            )
        });
        parsed["hookSpecificOutput"].clone()
    }

    /// The first stderr line holding anything but whitespace, trimmed at its start.
    pub fn first_stderr_line(&self) -> &str {
        self.stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(str::trim_start)
            .unwrap_or("")
    }
}

/// A temporary root with no "claude" in its path. Cargo's per-target directory comes first,
/// because the runner's `TMPDIR` may contain the name.
fn temp_dir_no_claude() -> TempDir {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let base = if base.to_string_lossy().to_lowercase().contains("claude") {
        std::env::temp_dir()
    } else {
        base
    };
    assert!(
        !base.to_string_lossy().to_lowercase().contains("claude"),
        "no temporary root without \"claude\" in its path ({})",
        base.display()
    );
    fs::create_dir_all(&base).expect("create temporary base");
    tempfile::Builder::new()
        .prefix("hook-false-positive-")
        .tempdir_in(&base)
        .expect("create temporary root")
}

pub fn write_exec(path: &Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod executable");
}

/// Write `stages/01-<stage_id>.md` declaring a running standard stage.
pub fn write_standard_stage(work_dir: &Path, stage_id: &str) {
    let stages = work_dir.join("stages");
    fs::create_dir_all(&stages).expect("create stages dir");
    let content = format!(
        "---\nid: {stage_id}\nname: Standard\nstatus: executing\nstage_type: standard\n\
         plan_id: PLAN-test\nworking_dir: .\n---\n\n# Stage: Standard\n"
    );
    fs::write(stages.join(format!("01-{stage_id}.md")), content).expect("write stage file");
}

/// Copy every regular file of `loom-hooks/` (its `tests/` directory excluded) into `dir`.
fn install_hooks(dir: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks");
    fs::create_dir_all(dir).expect("create hook dir");
    for entry in fs::read_dir(&source).expect("read loom-hooks") {
        let entry = entry.expect("loom-hooks entry");
        if entry.file_type().expect("entry type").is_file() {
            let target = dir.join(entry.file_name());
            fs::copy(entry.path(), &target).expect("copy hook");
            fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).expect("chmod hook");
        }
    }
    // loom-control-complete.sh honours LOOM_CONTROL_TEST_BIN only beside a `tests/` directory,
    // as in the repository checkout; it stays empty.
    fs::create_dir_all(dir.join("tests")).expect("create hook tests dir");
}

fn install_chain(dir: &Path) {
    fs::create_dir_all(dir).expect("create chain dir");
    write_exec(
        &dir.join("claude-outer.sh"),
        "#!/usr/bin/env bash\nexport LOOM_MAIN_AGENT_PID=$$\nbash \"$@\"\n",
    );
    for name in ["claude-mid.sh", "claude-inner.sh"] {
        write_exec(&dir.join(name), "#!/usr/bin/env bash\nbash \"$@\"\n");
    }
}

/// The fixture repository `root/repo`, its worktree `.worktrees/<stage>` (a real git worktree,
/// so `<repo>/.git/worktrees/<stage>` exists) and the worktree's `.loom/work` symlink that
/// `ensure_work_symlink` creates.
fn create_repo(root: &Path, stage: &str) -> (PathBuf, PathBuf) {
    let repo = root.join("repo");
    fs::create_dir_all(&repo).expect("create repo");
    test_support::init_git_repo(&repo);
    let relative = format!(".worktrees/{stage}");
    let branch = format!("loom/{stage}");
    test_support::git_checked(&repo, &["worktree", "add", "-q", "-b", &branch, &relative]);
    fs::create_dir_all(repo.join(".loom/work")).expect("create state dir");
    let wt = repo.join(relative);
    fs::create_dir_all(wt.join(".loom")).expect("create worktree .loom");
    symlink("../../../.loom/work", wt.join(".loom/work")).expect("link worktree state");
    (repo, wt)
}

pub struct Fixture {
    _temp: TempDir,
    pub root: PathBuf,
    pub hooks: PathBuf,
    pub repo: PathBuf,
    pub wt: PathBuf,
    pub tmpdir: PathBuf,
    pub home: PathBuf,
    pub stage: String,
}

impl Fixture {
    pub fn new(stage: &str) -> Self {
        let temp = temp_dir_no_claude();
        let root = fs::canonicalize(temp.path()).expect("physical root");
        install_hooks(&root.join("hooks"));
        install_chain(&root.join("chain"));
        let (repo, wt) = create_repo(&root, stage);
        let tmpdir = root.join("tmpdir");
        fs::create_dir_all(&tmpdir).expect("create tmpdir");
        fs::set_permissions(&tmpdir, fs::Permissions::from_mode(0o700)).expect("chmod tmpdir");
        let home = root.join("home");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(root.join("bin")).expect("create bin dir");
        write_exec(&root.join("bin/loom"), "#!/usr/bin/env bash\nexit 0\n");
        Fixture {
            hooks: root.join("hooks"),
            _temp: temp,
            root,
            repo,
            wt,
            tmpdir,
            home,
            stage: stage.to_string(),
        }
    }

    /// The executable `LOOM_CONTROL_TEST_BIN` names; the hook only compares its path.
    pub fn loom_bin(&self) -> PathBuf {
        self.root.join("bin/loom")
    }

    /// A uid-owned mode-700 scratch directory outside the repository.
    pub fn scratch(&self) -> PathBuf {
        let dir = self.root.join("scratch");
        fs::create_dir_all(&dir).expect("create scratch");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("chmod scratch");
        dir
    }

    /// `root/work`, holding a standard stage file for `stage`.
    pub fn work_dir(&self) -> PathBuf {
        let work = self.root.join("work");
        write_standard_stage(&work, &self.stage);
        work
    }

    /// Run `hook` on `payload` from `cwd`. Every ambient `LOOM_*` variable is scrubbed, TMPDIR is
    /// the fixture's empty tmpdir, HOME the fixture home, then `env` applies.
    pub fn run(
        &self,
        hook: &str,
        agent: Agent,
        mut payload: Value,
        cwd: &Path,
        env: &[(&str, String)],
    ) -> Run {
        let chain = self.root.join("chain");
        let mut command = Command::new("bash");
        match agent {
            Agent::Plain => {}
            Agent::Subagent(kind) => {
                payload["agent_type"] = json!(kind);
                command.args([
                    chain.join("claude-outer.sh"),
                    chain.join("claude-mid.sh"),
                    chain.join("claude-inner.sh"),
                ]);
            }
        }
        command.arg(self.hooks.join(hook));
        self.isolate(&mut command, cwd);
        for (key, value) in env {
            command.env(key, value);
        }
        Run::from_output(run_with_input(&mut command, payload.to_string().as_bytes()))
    }

    fn isolate(&self, command: &mut Command, cwd: &Path) {
        test_support::clear_loom_env(command);
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(key);
            }
        }
        // A production scratch directory is never inside a repository; the temporary root may
        // sit inside this checkout, so git discovery stops at the root.
        command
            .current_dir(cwd)
            .env("PWD", cwd)
            .env("TMPDIR", &self.tmpdir)
            .env("HOME", &self.home)
            .env("GIT_CEILING_DIRECTORIES", &self.root);
    }
}

/// A Bash tool payload whose `cwd` is `cwd`.
pub fn bash_payload(command: &str, cwd: &Path) -> Value {
    json!({"tool_name": "Bash", "tool_input": {"command": command}, "cwd": cwd})
}

/// `path` as an owned string, for env values.
pub fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Feed `input` from a file and capture both streams under a deadline.
pub fn run_with_input(command: &mut Command, input: &[u8]) -> Output {
    let mut payload = tempfile::tempfile().expect("create hook input");
    payload.write_all(input).expect("write hook input");
    payload.rewind().expect("rewind hook input");
    command
        .stdin(Stdio::from(payload))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    loom::process::run_bounded_output(command, Duration::from_secs(60), "hook contract")
        .unwrap_or_else(|error| panic!("run {command:?}: {error:#}"))
}
