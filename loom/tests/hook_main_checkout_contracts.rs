//! Contract for the worktree isolation hook's main-checkout rule: from a stage worktree, an
//! absolute path into the main checkout's own files is blocked, whether read directly or reached
//! through `cd`, while loom's state directory, system paths and the home directory stay open.

use serde_json::json;
use std::fs;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

const RULE_TAG: &str = "[worktree-isolation/main-checkout]";

/// Whether a temporary base would change what the hook decides: a `/.worktrees/` component (the
/// target directory of a stage worktree) makes every fixture path look like another stage's
/// worktree, and a `/.loom/` component marks a control input.
fn confuses_hook(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/.worktrees/") || text.contains("/.loom/")
}

fn temp_root() -> TempDir {
    let base = [
        std::env::temp_dir(),
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")),
    ]
    .into_iter()
    .find(|base| !confuses_hook(base))
    .unwrap_or_else(|| panic!("no temporary root the hook treats as an ordinary path"));
    fs::create_dir_all(&base).expect("create temporary base");
    tempfile::Builder::new()
        .prefix("hook-main-checkout-")
        .tempdir_in(&base)
        .expect("create temporary root")
}

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    repo: PathBuf,
    wt: PathBuf,
    home: PathBuf,
}

fn write_file(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("file has a parent")).expect("create parents");
    fs::write(path, body).expect("write fixture file");
}

impl Fixture {
    fn new() -> Self {
        let temp = temp_root();
        let root = fs::canonicalize(temp.path()).expect("physical root");
        let repo = root.join("repo");
        let wt = repo.join(".worktrees/stage-a");
        let home = root.join("home");
        write_file(&repo.join("src/main.rs"), "fn main() {}\n");
        write_file(&repo.join(".loom/work/signals/s.md"), "# Signal\n");
        fs::create_dir_all(&wt).expect("create worktree dir");
        fs::create_dir_all(home.join(".cargo/bin")).expect("create home cargo bin");
        fs::create_dir_all(root.join("tmpdir")).expect("create tmpdir");
        Fixture {
            _temp: temp,
            root,
            repo,
            wt,
            home,
        }
    }

    /// Run the hook on one Bash command from the worktree, with every `LOOM_*` and `GIT_*`
    /// variable removed and HOME set to the fixture home.
    fn run(&self, command_text: &str) -> (i32, String) {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/worktree-isolation.sh");
        let payload = json!({
            "tool_name": "Bash",
            "tool_input": {"command": command_text},
            "cwd": self.wt,
        });
        let mut command = Command::new("bash");
        command.arg(&script);
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.starts_with("LOOM_") || name.starts_with("GIT_") {
                command.env_remove(&key);
            }
        }
        command
            .current_dir(&self.wt)
            .env("PWD", &self.wt)
            .env("HOME", &self.home)
            .env("TMPDIR", self.root.join("tmpdir"))
            .env("GIT_CEILING_DIRECTORIES", &self.root);
        let mut input = tempfile::tempfile().expect("create hook input");
        input
            .write_all(payload.to_string().as_bytes())
            .expect("write hook input");
        input.rewind().expect("rewind hook input");
        command
            .stdin(Stdio::from(input))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output =
            loom::process::run_bounded_output(&mut command, Duration::from_secs(60), "hook")
                .unwrap_or_else(|error| panic!("run {command:?}: {error:#}"));
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        (output.status.code().unwrap_or(-1), stderr)
    }
}

fn first_line(stderr: &str) -> &str {
    stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim_start)
        .unwrap_or("")
}

#[test]
fn an_absolute_path_into_the_main_checkout_is_blocked() {
    let fx = Fixture::new();
    let repo = fx.repo.display();
    let home = fx.home.display();
    let blocked = [
        format!("cat {repo}/src/main.rs"),
        format!("cd {repo}/src && cat main.rs"),
    ];
    for command in &blocked {
        let (code, stderr) = fx.run(command);
        assert_eq!(code, 2, "not blocked: {command}\nstderr: {stderr}");
        assert!(
            first_line(&stderr).starts_with(RULE_TAG),
            "blocked without the {RULE_TAG} tag: {command}\nstderr: {stderr}"
        );
    }
    let allowed = [
        format!("cat {repo}/.loom/work/signals/s.md"),
        "cat /etc/hostname".to_string(),
        format!("ls {home}/.cargo/bin"),
    ];
    for command in &allowed {
        let (code, stderr) = fx.run(command);
        assert_eq!(code, 0, "blocked: {command}\nstderr: {stderr}");
    }
}
