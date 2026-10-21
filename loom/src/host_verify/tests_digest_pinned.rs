//! The host side (the child, the daemon applying a freeze) reads a stage
//! worktree's HEAD and digest through its registered git directory, whatever
//! the worktree's `.git` file names, and agrees with the session's own reads
//! of an honest tree.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use serial_test::serial;
use tempfile::TempDir;

use super::child::JobTree;
use super::digest::{head, head_in, tree_digest, tree_digest_in};
use super::{new_job_id, JobKind, JobRecord, JobState};
use crate::git::worktree::WorktreeGit;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};

const STAGE: &str = "s1";

/// A stage worktree whose `.git` file the stage repointed at a git directory
/// it made, whose clean filter creates `marker` whenever git runs it.
struct Repointed {
    _tmp: TempDir,
    repo: PathBuf,
    worktree: PathBuf,
    marker: PathBuf,
    registered: Vec<u8>,
    planted: Vec<u8>,
}

impl Repointed {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let worktree = contract_worktree(&repo, STAGE);
        let registered = fs::read(worktree.join(".git")).unwrap();
        let marker = plant_foreign_git_dir(&worktree);
        let planted = fs::read(worktree.join(".git")).unwrap();
        Self {
            _tmp: tmp,
            repo,
            worktree,
            marker,
            registered,
            planted,
        }
    }

    /// HEAD and digest as the session reads them with loom's own `.git`
    /// file in place; the repointed file is back afterwards.
    fn honest(&self) -> (String, String) {
        let git_file = self.worktree.join(".git");
        fs::write(&git_file, &self.registered).unwrap();
        let read = (
            head(&self.worktree).unwrap(),
            tree_digest(&self.worktree).unwrap(),
        );
        fs::write(&git_file, &self.planted).unwrap();
        read
    }

    /// The positive control, run last: git that follows the repointed file
    /// does run the filter, so its absence before comes from the pin.
    fn assert_filter_live(&self) {
        let _ = tree_digest(&self.worktree);
        assert!(self.marker.exists(), "the fixture's filter never runs");
    }
}

fn record(head: String, tree_digest: String) -> JobRecord {
    let now = Utc::now();
    JobRecord {
        id: new_job_id(),
        stage_id: STAGE.to_string(),
        session_id: "session-a".to_string(),
        kind: JobKind::Check,
        head,
        tree_digest,
        no_cache: true,
        state: JobState::Running,
        requests: vec!["r1".to_string()],
        queued_at: now,
        started_at: Some(now),
        finished_at: None,
        pid: None,
        detail: None,
    }
}

#[test]
fn pinned_reads_ignore_a_repointed_git_file() {
    let fx = Repointed::new();
    let pinned = WorktreeGit::pinned(&fx.repo, &fx.worktree).unwrap();

    let host = (head_in(&pinned).unwrap(), tree_digest_in(&pinned).unwrap());

    assert!(
        !fx.marker.exists(),
        "the host digest ran the stage's filter"
    );
    assert_eq!(host, fx.honest());
    fx.assert_filter_live();
}

#[test]
fn the_host_child_checks_the_registered_tree_whatever_the_git_file_names() {
    let fx = Repointed::new();
    let (head, digest) = fx.honest();
    let stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        status: StageStatus::Executing,
        worktree: Some(STAGE.to_string()),
        working_dir: Some(".".to_string()),
        stage_type: StageType::Standard,
        ..Default::default()
    };
    let work_dir = fx.repo.join(".loom").join("work");

    let tree = JobTree::resolve(&stage, &work_dir).unwrap();
    let differences = tree.differences(&record(head, digest)).unwrap();

    assert!(!fx.marker.exists(), "the host child ran the stage's filter");
    assert_eq!(differences, None);
    fx.assert_filter_live();
}

/// The session reads as git discovers the worktree, the host through the pin;
/// both must name the same tree with the same digest (common.md 3.5).
#[test]
fn session_and_host_reads_agree_on_an_honest_tree() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, STAGE);
    fs::write(worktree.join("README.md"), "edited\n").unwrap();
    fs::write(worktree.join("new.txt"), "new\n").unwrap();
    let pinned = WorktreeGit::pinned(&repo, &worktree).unwrap();

    assert_eq!(
        tree_digest(&worktree).unwrap(),
        tree_digest_in(&pinned).unwrap()
    );
    assert_eq!(head(&worktree).unwrap(), head_in(&pinned).unwrap());
}

const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

/// Points `XDG_CONFIG_HOME` at a directory for one serialized test and
/// restores it, even when the test fails.
struct XdgConfigHome(Option<OsString>);

impl XdgConfigHome {
    fn set(dir: &Path) -> Self {
        let previous = std::env::var_os(XDG_CONFIG_HOME);
        // SAFETY: serialized test; restored on drop.
        unsafe { std::env::set_var(XDG_CONFIG_HOME, dir) };
        Self(previous)
    }
}

impl Drop for XdgConfigHome {
    fn drop(&mut self) {
        // SAFETY: serialized test; puts back what `set` replaced.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var(XDG_CONFIG_HOME, value),
                None => std::env::remove_var(XDG_CONFIG_HOME),
            }
        }
    }
}

/// The session and the daemon run with different `HOME`s, so git's default
/// excludes file differs between them; an untracked file the host's lists
/// must not move the digest. The positive control, run last, shows git with
/// that excludes file hides the file.
#[test]
#[serial]
fn a_default_excludes_file_does_not_change_the_digest() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, STAGE);
    fs::write(worktree.join("scratch.log"), "local\n").unwrap();
    let pinned = WorktreeGit::pinned(&repo, &worktree).unwrap();
    let (plain, ignoring) = (
        tmp.path().join("xdg-plain"),
        tmp.path().join("xdg-ignoring"),
    );
    fs::create_dir_all(plain.join("git")).unwrap();
    fs::create_dir_all(ignoring.join("git")).unwrap();
    fs::write(ignoring.join("git").join("ignore"), "scratch.log\n").unwrap();

    let session = {
        let _xdg = XdgConfigHome::set(&plain);
        tree_digest(&worktree).unwrap()
    };
    let host = {
        let _xdg = XdgConfigHome::set(&ignoring);
        tree_digest_in(&pinned).unwrap()
    };

    assert_eq!(session, host);
    let listed = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .env(XDG_CONFIG_HOME, &ignoring)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(&worktree)
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&listed.stdout);
    assert!(listed.contains("x_contract.rs"), "{listed}");
    assert!(
        !listed.contains("scratch.log"),
        "the excludes file is not live: {listed}"
    );
}
