//! Fixture repositories, fake `claude` scripts and a runner for the
//! `loom knowledge bootstrap` integration tests.
//!
//! Every command built here runs with git's global config and the XDG config
//! home isolated, and with `PATH` limited to the fake's directory, the real
//! git's directory, `/usr/bin` and `/bin`: `find_claude_path` falls back to
//! `~/.claude/local/claude`, so a fake that is not found must never let a
//! real, billed session start.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

use super::helpers::{init_test_repo, loom_bin_path, loom_cmd};

/// Logs its invocation and exits 0 without touching the marker.
pub const LOG_AND_EXIT: &str = "#!/bin/sh\nprintf 'invoked\\n' >> \"$FAKE_LOG\"\nexit 0\n";

/// Exits 1 without touching the marker.
pub const EXIT_1: &str = "#!/bin/sh\nexit 1\n";

/// Touches the marker and exits at once: the marker-then-exit race.
pub const MARKER_THEN_EXIT: &str =
    "#!/bin/sh\ntouch \".loom/work/bootstrap/claude-$PPID.done\"\nexit 0\n";

/// Logs, records its argv, writes one entry into every tier-1 file, copies
/// the brief when `FAKE_BRIEF` is set, touches the marker, then waits for
/// loom's SIGTERM. `$PPID` is the loom process, so the marker path is exact
/// and each run's headings are unique.
pub const COMPLETING_FAKE: &str = r###"#!/bin/sh
printf 'invoked\n' >> "$FAKE_LOG"
printf '%s\n' "$@" > "$FAKE_ARGS"
for f in architecture entry-points patterns conventions mistakes stack concerns; do
  "$LOOM_BIN" knowledge update "$f" "## Fake $f Entry $PPID" || exit 1
done
if [ -n "$FAKE_BRIEF" ]; then
  cat ".loom/work/bootstrap/brief-$PPID.md" >> "$FAKE_BRIEF"
fi
touch ".loom/work/bootstrap/claude-$PPID.done"
exec sleep 30
"###;

/// A fake `claude` plus the files it writes, all outside any fixture repo.
pub struct Sandbox {
    aux: TempDir,
}

impl Sandbox {
    /// Install `script` as `claude` in a private bin directory.
    pub fn new(script: &str) -> Self {
        let aux = TempDir::new().expect("create sandbox dir");
        fs::create_dir_all(aux.path().join("bin")).expect("create fake bin dir");
        fs::create_dir_all(aux.path().join("xdg")).expect("create XDG config home");
        let fake = aux.path().join("bin").join("claude");
        fs::write(&fake, script).expect("write fake claude");
        fs::set_permissions(fake, fs::Permissions::from_mode(0o755)).expect("chmod fake");
        Sandbox { aux }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.aux.path().join(name)
    }

    fn isolate<'c>(&self, command: &'c mut Command) -> &'c mut Command {
        let path = format!(
            "{}:{}:/usr/bin:/bin",
            self.file("bin").display(),
            real_git_dir().display()
        );
        command
            .env("PATH", path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("XDG_CONFIG_HOME", self.file("xdg"))
            .env("FAKE_LOG", self.file("log"))
            .env("FAKE_ARGS", self.file("args"))
            .env("FAKE_BRIEF", self.file("brief"))
    }

    /// `loom knowledge bootstrap <args>` in `repo`, not yet run.
    pub fn command(&self, repo: &Path, args: &[&str]) -> Command {
        let mut command = loom_cmd();
        command.env("LOOM_BIN", loom_bin_path());
        self.isolate(&mut command);
        command
            .current_dir(repo)
            .args(["knowledge", "bootstrap"])
            .args(args);
        command
    }

    pub fn bootstrap(&self, repo: &Path, args: &[&str]) -> Output {
        self.command(repo, args)
            .output()
            .expect("run loom knowledge bootstrap")
    }

    /// Run git in `dir`, asserting success; returns stdout.
    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let mut command = Command::new("git");
        self.isolate(&mut command);
        let output = command
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// `init_test_repo()` plus `files`, each holding one small function,
    /// committed.
    pub fn fixture(&self, files: &[String]) -> TempDir {
        let repo = init_test_repo();
        for (index, file) in files.iter().enumerate() {
            let path = repo.path().join(file);
            fs::create_dir_all(path.parent().expect("file has a parent")).expect("mkdir");
            fs::write(
                path,
                format!("pub fn f{index}() -> usize {{\n    {index}\n}}\n"),
            )
            .expect("write fixture file");
        }
        self.git(repo.path(), &["add", "-A"]);
        self.git(repo.path(), &["commit", "-qm", "fixture"]);
        repo
    }

    /// How many times a fake logged an invocation.
    pub fn invocations(&self) -> usize {
        fs::read_to_string(self.file("log")).map_or(0, |log| log.lines().count())
    }

    pub fn log_exists(&self) -> bool {
        self.file("log").exists()
    }

    /// The argv the completing fake recorded, one line per `printf` line.
    pub fn recorded_args(&self) -> Vec<String> {
        let args = fs::read_to_string(self.file("args")).expect("fake recorded its argv");
        args.lines().map(str::to_string).collect()
    }

    /// Every brief the completing fake copied.
    pub fn briefs(&self) -> String {
        fs::read_to_string(self.file("brief")).expect("fake copied the brief")
    }
}

/// The directory of the first executable `git` on the test process's `PATH`.
fn real_git_dir() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH is set");
    std::env::split_paths(&path)
        .find(|dir| {
            dir.join("git")
                .metadata()
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
        .expect("git on PATH")
}

/// `dir/<prefix>NN.rs` for each `NN` in `range`.
pub fn numbered(dir: &str, prefix: &str, range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|n| format!("{dir}/{prefix}{n:02}.rs")).collect()
}

/// 46 files on top of `README.md`: clusters `.`, `src`, `src/core`, `src/util`.
pub fn core_util_files() -> Vec<String> {
    let mut files = vec!["src/lib.rs".to_string(), "src/util/a.rs".to_string()];
    files.extend(numbered("src/core", "c", 0..35));
    files.extend(numbered("src/util", "u", 1..10));
    files
}

/// stdout and stderr, asserting a zero exit.
pub fn success(output: &Output) -> String {
    let text = combined(output);
    assert!(output.status.success(), "bootstrap failed:\n{text}");
    text
}

pub fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Whitespace-split rows of the report's cluster table (header excluded).
pub fn table_rows(stdout: &str) -> Vec<Vec<String>> {
    stdout
        .lines()
        .skip_while(|line| !line.starts_with("cluster "))
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .map(|line| line.split_whitespace().map(str::to_string).collect())
        .collect()
}

pub fn table_ids(stdout: &str) -> Vec<String> {
    table_rows(stdout)
        .into_iter()
        .map(|row| row[0].clone())
        .collect()
}

/// Cluster ids in the committed receipt, or `None` when there is none.
pub fn receipt_ids(repo: &Path) -> Option<Vec<String>> {
    let path = repo.join("doc/loom/knowledge/.bootstrap-receipt.json");
    let text = fs::read_to_string(path).ok()?;
    let receipt: serde_json::Value = serde_json::from_str(&text).expect("receipt is JSON");
    let clusters = receipt["clusters"].as_array().expect("clusters array");
    Some(
        clusters
            .iter()
            .map(|cluster| cluster["id"].as_str().expect("cluster id").to_string())
            .collect(),
    )
}

/// Briefs left in `.loom/work/bootstrap/`.
pub fn leftover_briefs(repo: &Path) -> usize {
    let Ok(entries) = fs::read_dir(repo.join(".loom/work/bootstrap")) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("brief-"))
        .count()
}
