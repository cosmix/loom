//! Fixture repositories, planning runtime directories and a fake `loom` executable for
//! the run-handoff contracts.
#![allow(dead_code)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use loom::planning::runtime::{create_runtime_dir, runtime_paths_under, RuntimePaths};
use tempfile::TempDir;

/// The running plan every abandon fixture holds.
pub const PLAN: &str = "doc/plans/IN_PROGRESS-PLAN-a.md";
/// Where a successful abandon moves [`PLAN`].
pub const RETIRED: &str = "doc/plans/archive/RETIRED-PLAN-a.md";

/// A git repository at `<tmp>/repo` with one commit on `main`, and a loom user
/// directory at `<tmp>/home`.
pub struct Repo {
    pub temp: TempDir,
    pub root: PathBuf,
    pub loom_dir: PathBuf,
}

impl Repo {
    pub fn new() -> Repo {
        let temp = tempfile::Builder::new()
            .prefix("loom-handoff-contracts-")
            .tempdir()
            .expect("create scratch directory");
        let root = temp.path().join("repo");
        let loom_dir = temp.path().join("home");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&loom_dir).unwrap();
        let repo = Repo {
            temp,
            root,
            loom_dir,
        };
        repo.git(&["init", "-q"]);
        repo.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        // Only the hooks a test installs may run, whatever the user's git config says.
        let hooks = repo.root.join(".git/hooks");
        fs::remove_dir_all(&hooks).ok();
        fs::create_dir_all(&hooks).unwrap();
        let hooks_path = hooks.display().to_string();
        for (key, value) in [
            ("user.name", "Handoff Contracts"),
            ("user.email", "handoff@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", hooks_path.as_str()),
        ] {
            repo.git(&["config", key, value]);
        }
        repo.put("README.md", "# Fixture\n");
        repo.commit_all("initial");
        repo
    }

    /// A repository whose committed [`PLAN`] runs in a `.loom/work` workspace.
    pub fn in_progress_run() -> Repo {
        let repo = Repo::new();
        repo.put(PLAN, "# A\n");
        repo.commit_all("plan a");
        repo.workspace(PLAN);
        repo
    }

    /// A repository whose second commit holds `files`.
    pub fn committed(files: &[(&str, &str)]) -> Repo {
        let repo = Repo::new();
        repo.put_all(files);
        repo.commit_all("fixture files");
        repo
    }

    pub fn put_all(&self, files: &[(&str, &str)]) {
        for (rel, content) in files {
            self.put(rel, content);
        }
    }

    /// Whether `rel` is a regular file in the repository.
    pub fn has(&self, rel: &str) -> bool {
        self.root.join(rel).is_file()
    }

    /// Runs git in the repository, panicking on failure; returns stdout.
    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("git prints UTF-8")
    }

    pub fn put(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    pub fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// The paths the HEAD commit changed.
    pub fn head_files(&self) -> Vec<String> {
        lines(&self.git(&["show", "--name-only", "--format=", "HEAD"]))
    }

    /// The paths staged in the index.
    pub fn staged(&self) -> Vec<String> {
        lines(&self.git(&["diff", "--cached", "--name-only"]))
    }

    pub fn status(&self) -> String {
        self.git(&["status", "--porcelain", "--untracked-files=all"])
    }

    /// Points `branch` at a chain of `ahead` new commits on top of HEAD, leaving the
    /// checkout on `main`.
    pub fn branch_ahead(&self, branch: &str, ahead: usize) {
        let mut tip = self.head();
        for n in 0..ahead {
            let message = format!("{branch} commit {n}");
            let args = ["commit-tree", "-p", &tip, "-m", &message, "HEAD^{tree}"];
            tip = self.git(&args).trim().to_string();
        }
        self.git(&["update-ref", &format!("refs/heads/{branch}"), &tip]);
    }

    pub fn has_branch(&self, branch: &str) -> bool {
        Command::new("git")
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"))
            .current_dir(&self.root)
            .output()
            .expect("run git")
            .status
            .success()
    }

    /// Writes `.loom/work/config.toml` naming `source` as the plan, merging into `main`.
    pub fn workspace(&self, source: &str) {
        let work = self.root.join(".loom/work");
        fs::create_dir_all(work.join("stages")).unwrap();
        fs::create_dir_all(work.join("sessions")).unwrap();
        let config = format!(
            "[plan]\nsource_path = \"{source}\"\nplan_id = \"a\"\nplan_name = \"A\"\nbase_branch = \"main\"\n"
        );
        fs::write(work.join("config.toml"), config).unwrap();
    }

    /// Installs an executable git hook.
    pub fn hook(&self, name: &str, script: &str) {
        let path = self.root.join(".git/hooks").join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The server's runtime paths for this repository, its directory created.
    pub fn runtime(&self) -> RuntimePaths {
        let runtime = runtime_paths_under(&self.loom_dir, &self.root).expect("runtime paths");
        create_runtime_dir(&runtime).expect("create the runtime directory");
        runtime
    }

    pub fn marker_path(&self, runtime: &RuntimePaths) -> PathBuf {
        runtime.dir.join("abandon").join("a.json")
    }

    /// Writes the abandon marker an interrupted abandon of [`PLAN`] leaves.
    pub fn write_marker(&self, runtime: &RuntimePaths) {
        let dir = runtime.dir.join("abandon");
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let path = self.marker_path(runtime);
        let body = format!("{{\"plan\":\"{PLAN}\",\"started_at\":\"2026-10-29T00:00:00Z\"}}\n");
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    /// Every entry under `doc/plans/archive/`, none when it does not exist.
    pub fn archive_entries(&self) -> Vec<PathBuf> {
        match fs::read_dir(self.root.join("doc/plans/archive")) {
            Ok(entries) => entries.map(|e| e.unwrap().path()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// The fake `loom`: records every invocation, exits with `exit-<subcommand>` when
/// present, dumps `LOOM_CLEAN_YES` on `clean` and deletes `.loom/work` when
/// `clean-deletes-work` exists, and runs `stop.sh` (with `$ROOT` set) on `stop`.
const FAKE_LOOM: &str = r#"#!/bin/sh
d=$(dirname "$0")
ROOT=$(cat "$d/root")
export ROOT
echo "$*" >> "$d/invocations"
case "$1" in
  clean)
    echo "LOOM_CLEAN_YES=${LOOM_CLEAN_YES-<unset>}" > "$d/clean.env"
    if [ -f "$d/clean-deletes-work" ]; then rm -rf "$ROOT/.loom/work"; fi
    ;;
  stop)
    if [ -f "$d/stop.sh" ]; then sh "$d/stop.sh" || exit 97; fi
    ;;
esac
if [ -f "$d/exit-$1" ]; then exit "$(cat "$d/exit-$1")"; fi
exit 0
"#;

pub struct FakeLoom {
    pub dir: PathBuf,
    pub bin: PathBuf,
}

impl FakeLoom {
    pub fn new(repo: &Repo) -> FakeLoom {
        let dir = repo.temp.path().join("fake");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("root"), repo.root.display().to_string()).unwrap();
        let bin = dir.join("loom");
        fs::write(&bin, FAKE_LOOM).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        FakeLoom { dir, bin }
    }

    pub fn exit(&self, subcommand: &str, code: i32) {
        fs::write(
            self.dir.join(format!("exit-{subcommand}")),
            code.to_string(),
        )
        .unwrap();
    }

    pub fn clean_deletes_work(&self) {
        fs::write(self.dir.join("clean-deletes-work"), "").unwrap();
    }

    /// A shell script `stop` runs before it exits; `$ROOT` names the repository.
    pub fn on_stop(&self, script: &str) {
        fs::write(self.dir.join("stop.sh"), script).unwrap();
    }

    /// Each invocation's arguments, in order.
    pub fn invocations(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("invocations"))
            .map(|text| lines(&text))
            .unwrap_or_default()
    }

    pub fn ran(&self, subcommand: &str) -> bool {
        self.invocations()
            .iter()
            .any(|call| call.split_whitespace().next() == Some(subcommand))
    }

    /// The `LOOM_CLEAN_YES=...` line the last `clean` recorded.
    pub fn clean_env(&self) -> Option<String> {
        fs::read_to_string(self.dir.join("clean.env"))
            .ok()
            .map(|text| text.trim().to_string())
    }
}

/// Sets an environment variable until dropped, then restores its old value.
pub struct EnvGuard {
    key: &'static str,
    old: Option<OsString>,
}

impl EnvGuard {
    pub fn set(key: &'static str, value: &Path) -> EnvGuard {
        let old = std::env::var_os(key);
        std::env::set_var(key, value);
        EnvGuard { key, old }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.old {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}
