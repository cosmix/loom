//! The abandon sequence on fixture repositories with a fake `loom` executable.

mod branches;
mod retire;
mod route;

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serial_test::serial;
use tempfile::TempDir;

use super::sequence::CLEAN_LEFT_STATE;
use super::{run_abandon, stranded_plan, AbandonOutcome};
use crate::planning::runtime::{create_runtime_dir, runtime_paths_under, RuntimePaths};

const PLAN: &str = "doc/plans/IN_PROGRESS-PLAN-a.md";
const RETIRED: &str = "doc/plans/archive/RETIRED-PLAN-a.md";

/// Records every invocation with the `LOOM_CLEAN_YES` it saw, runs `<subcommand>.sh` (with
/// `$ROOT` set) and exits with `exit-<subcommand>` when present.
const FAKE_LOOM: &str = r#"#!/bin/sh
d=$(dirname "$0")
ROOT=$(cat "$d/root")
export ROOT
echo "$1 LOOM_CLEAN_YES=${LOOM_CLEAN_YES-unset}" >> "$d/calls"
if [ -f "$d/$1.sh" ]; then sh "$d/$1.sh" || exit 97; fi
if [ -f "$d/exit-$1" ]; then exit "$(cat "$d/exit-$1")"; fi
exit 0
"#;

/// Removes the workspace the way a successful `loom clean --all` does.
const CLEAN_REMOVES_WORK: &str = "rm -rf \"$ROOT/.loom/work\"\n";

pub(super) struct Repo {
    pub temp: TempDir,
    pub root: PathBuf,
    pub fake: PathBuf,
}

impl Repo {
    /// A git repository on `main` with one commit, a fake `loom`, and no workspace.
    pub fn new() -> Repo {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let fake = temp.path().join("fake");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&fake).unwrap();
        fs::write(fake.join("root"), root.display().to_string()).unwrap();
        fs::write(fake.join("loom"), FAKE_LOOM).unwrap();
        fs::set_permissions(fake.join("loom"), fs::Permissions::from_mode(0o755)).unwrap();
        let repo = Repo { temp, root, fake };
        repo.git(&["init", "-q"]);
        repo.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        let hooks = repo.temp.path().join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hooks = hooks.display().to_string();
        for (key, value) in [
            ("user.name", "Abandon Tests"),
            ("user.email", "abandon@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", hooks.as_str()),
        ] {
            repo.git(&["config", key, value]);
        }
        repo.put("README.md", "# Fixture\n");
        repo.commit_all("initial");
        repo
    }

    /// A repository whose committed [`PLAN`] runs in a `.loom/work` workspace.
    pub fn running() -> Repo {
        let repo = Repo::new();
        repo.put(PLAN, "# A\n");
        repo.commit_all("plan a");
        repo.workspace(PLAN, "main");
        repo
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "git {args:?}: {stderr}");
        String::from_utf8(output.stdout).unwrap()
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

    /// Points `branch` at `ahead` new commits on top of HEAD.
    pub fn branch_ahead(&self, branch: &str, ahead: usize) {
        let mut tip = self.git(&["rev-parse", "HEAD"]).trim().to_string();
        for n in 0..ahead {
            let message = format!("{branch} {n}");
            let args = ["commit-tree", "-p", &tip, "-m", &message, "HEAD^{tree}"];
            tip = self.git(&args).trim().to_string();
        }
        self.git(&["update-ref", &format!("refs/heads/{branch}"), &tip]);
    }

    /// `.loom/work/config.toml` naming `source` as the plan, merging into `base`.
    pub fn workspace(&self, source: &str, base: &str) {
        let work = self.root.join(".loom/work");
        fs::create_dir_all(work.join("stages")).unwrap();
        fs::create_dir_all(work.join("sessions")).unwrap();
        let config = format!(
            "[plan]\nsource_path = \"{source}\"\nplan_id = \"a\"\nplan_name = \"A\"\nbase_branch = \"{base}\"\n"
        );
        fs::write(work.join("config.toml"), config).unwrap();
    }

    pub fn runtime(&self) -> RuntimePaths {
        let runtime = runtime_paths_under(&self.temp.path().join("home"), &self.root).unwrap();
        create_runtime_dir(&runtime).unwrap();
        runtime
    }

    pub fn marker(&self, runtime: &RuntimePaths) -> PathBuf {
        runtime.dir.join("abandon/a.json")
    }

    pub fn has(&self, rel: &str) -> bool {
        self.root.join(rel).is_file()
    }

    /// The fake's behaviour for `subcommand`: a shell script it runs.
    pub fn on(&self, subcommand: &str, script: &str) {
        fs::write(self.fake.join(format!("{subcommand}.sh")), script).unwrap();
    }

    pub fn exit(&self, subcommand: &str, code: i32) {
        fs::write(
            self.fake.join(format!("exit-{subcommand}")),
            code.to_string(),
        )
        .unwrap();
    }

    /// Each invocation as `<subcommand> LOOM_CLEAN_YES=<value>`.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.fake.join("calls"))
            .map(|text| text.lines().map(String::from).collect())
            .unwrap_or_default()
    }

    pub fn abandon(&self, runtime: &RuntimePaths, confirmed: &[(String, usize)]) -> AbandonOutcome {
        run_abandon(
            &self.root,
            runtime,
            &self.fake.join("loom"),
            confirmed,
            &mut |_| {},
        )
        .unwrap()
    }

    /// The sequence with no unmerged branch confirmed, and every output line it reported.
    pub fn abandon_lines(&self, runtime: &RuntimePaths) -> (AbandonOutcome, Vec<String>) {
        let mut lines = Vec::new();
        let bin = self.fake.join("loom");
        let mut on_line = |line: &str| lines.push(line.to_string());
        let outcome = run_abandon(&self.root, runtime, &bin, &[], &mut on_line).unwrap();
        (outcome, lines)
    }
}

/// Sets an environment variable until dropped.
pub(super) struct EnvGuard(&'static str, Option<OsString>);

impl EnvGuard {
    pub fn set(key: &'static str, value: &Path) -> EnvGuard {
        let old = std::env::var_os(key);
        std::env::set_var(key, value);
        EnvGuard(key, old)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.1 {
            Some(value) => std::env::set_var(self.0, value),
            None => std::env::remove_var(self.0),
        }
    }
}

#[test]
#[serial]
fn clean_exiting_zero_but_leaving_the_workspace_keeps_the_name() {
    let repo = Repo::running();
    let runtime = repo.runtime();
    let (outcome, lines) = repo.abandon_lines(&runtime);
    assert!(
        matches!(outcome, AbandonOutcome::Refused { .. }),
        "{outcome:?}"
    );
    assert!(repo.has(PLAN));
    assert!(!repo.root.join("doc/plans/archive").exists());
    assert!(
        repo.marker(&runtime).exists(),
        "clean ran: the marker must stay"
    );
    assert_eq!(lines.last().map(String::as_str), Some(CLEAN_LEFT_STATE));
}

#[test]
#[serial]
fn a_failing_stop_runs_nothing_after_it() {
    let repo = Repo::running();
    repo.exit("stop", 3);
    let runtime = repo.runtime();
    let outcome = repo.abandon(&runtime, &[]);
    let step = "loom stop".to_string();
    assert_eq!(outcome, AbandonOutcome::Failed { step, code: 3 });
    assert_eq!(repo.calls(), vec!["stop LOOM_CLEAN_YES=unset"]);
    assert!(
        !repo.marker(&runtime).exists(),
        "nothing changed: the marker must go"
    );
    assert!(repo.has(PLAN));
}

#[test]
#[serial]
fn a_failing_clean_stops_with_no_rename() {
    let repo = Repo::running();
    repo.exit("clean", 1);
    let runtime = repo.runtime();
    let outcome = repo.abandon(&runtime, &[]);
    let step = "loom clean".to_string();
    assert_eq!(outcome, AbandonOutcome::Failed { step, code: 1 });
    assert!(repo.has(PLAN));
    assert!(!repo.root.join(RETIRED).exists());
    assert!(!repo.marker(&runtime).exists());
}

#[test]
#[serial]
fn a_successful_sequence_renames_into_a_created_archive() {
    let repo = Repo::running();
    repo.on("clean", CLEAN_REMOVES_WORK);
    let runtime = repo.runtime();
    assert!(!repo.root.join("doc/plans/archive").exists());
    let (outcome, lines) = repo.abandon_lines(&runtime);
    assert_eq!(
        outcome,
        AbandonOutcome::Renamed {
            to: PathBuf::from(RETIRED)
        }
    );
    assert!(repo.has(RETIRED));
    assert!(!repo.root.join(PLAN).exists());
    let moved = format!("Moved {PLAN} to {RETIRED}.");
    assert_eq!(lines.last(), Some(&moved));
    assert!(
        !repo.marker(&runtime).exists(),
        "the marker outlived the rename"
    );
}

#[test]
#[serial]
fn a_marker_written_under_the_given_runtime_is_found_with_another_plan_state() {
    let repo = Repo::running();
    // The rename refuses after clean: the target appears while clean runs.
    let retired = repo.root.join(RETIRED).display().to_string();
    repo.on(
        "clean",
        &format!(
            "{CLEAN_REMOVES_WORK}mkdir -p \"$(dirname '{retired}')\" && echo x > '{retired}'\n"
        ),
    );
    let runtime = repo.runtime();
    let other = tempfile::tempdir().unwrap();
    let _state = EnvGuard::set("LOOM_PLAN_STATE", other.path());
    let outcome = repo.abandon(&runtime, &[]);
    assert!(
        matches!(outcome, AbandonOutcome::Refused { .. }),
        "{outcome:?}"
    );
    assert!(repo.marker(&runtime).exists());
    assert_eq!(stranded_plan(&repo.root, &runtime).as_deref(), Some(PLAN));
}
