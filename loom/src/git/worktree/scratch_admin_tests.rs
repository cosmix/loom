//! Adversarial tests of pinned git in a repository with
//! `extensions.worktreeConfig` on: a `config.worktree` a session rewrites at any
//! moment is never read, and what git writes still reaches the real worktree.

use super::files::PRIVATE_PARENT;
use super::*;
use crate::git::runner::{run_git, run_git_with_env};
use crate::git::worktree::{partition_admin_dirs, WorktreeGit};
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

const SPARSE: &str = "[core]\n\tsparseCheckout = true\n";

struct Fixture {
    _tmp: TempDir,
    repo: PathBuf,
    worktree: PathBuf,
    admin: PathBuf,
    common: PathBuf,
    marker: PathBuf,
}

impl Fixture {
    /// `<tmp>/repo` with the worktree `.worktrees/s1` and the extension on.
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let worktree = contract_worktree(&repo, "s1");
        ok(&repo, &["config", "extensions.worktreeConfig", "true"]);
        ok(&repo, &["config", "user.name", "Test"]);
        ok(&repo, &["config", "user.email", "test@example.invalid"]);
        ok(&repo, &["config", "commit.gpgsign", "false"]);
        let common = repo.join(".git").canonicalize().unwrap();
        Self {
            admin: common.join("worktrees/s1"),
            marker: tmp.path().join("filter-ran"),
            _tmp: tmp,
            repo,
            worktree,
            common,
        }
    }

    fn pinned(&self) -> WorktreeGit {
        WorktreeGit::pinned(&self.repo, &self.worktree).unwrap()
    }

    fn hostile_config(&self) -> String {
        format!(
            "[filter \"x\"]\n\tclean = touch '{}'; cat\n",
            self.marker.display()
        )
    }

    /// A file whose re-hash on every `git status` runs the filter `x`: the
    /// index holds no stat data for it and its content differs from `HEAD`.
    fn make_status_hash(&self) {
        ok(&self.worktree, &["read-tree", "HEAD"]);
        std::fs::write(self.worktree.join(".gitattributes"), "* filter=x\n").unwrap();
        std::fs::write(self.worktree.join("README.md"), "changed\n").unwrap();
    }

    /// `git status` with `GIT_DIR` naming `git_dir`.
    fn status_with_git_dir(&self, git_dir: &Path) -> std::process::Output {
        let env = [
            ("GIT_DIR", git_dir.as_os_str()),
            ("GIT_COMMON_DIR", self.common.as_os_str()),
            ("GIT_WORK_TREE", self.worktree.as_os_str()),
        ];
        run_git_with_env(&["status", "--porcelain"], &env, &self.worktree).unwrap()
    }

    fn scratch(&self) -> ScratchAdmin {
        ScratchAdmin::prepare(&self.repo, &self.admin, &self.common, true).unwrap()
    }
}

fn ok(dir: &Path, args: &[&str]) -> String {
    let output = run_git(args, dir).unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// A swap exactly between validation and use. The positive control runs git
/// on the real directory at the same moment: that does run the filter.
#[test]
fn a_config_worktree_swapped_after_validation_is_never_read() {
    let fixture = Fixture::new();
    fixture.make_status_hash();
    let file = fixture.admin.join(CONFIG_WORKTREE);
    std::fs::write(&file, SPARSE).unwrap();

    let scratch = fixture.scratch();
    std::fs::write(&file, fixture.hostile_config()).unwrap();

    let output = fixture.status_with_git_dir(scratch.path());
    assert!(output.status.success(), "{output:?}");
    assert!(!fixture.marker.exists(), "git read the swapped file");

    let control = fixture.status_with_git_dir(&fixture.admin);
    assert!(control.status.success(), "{control:?}");
    assert!(fixture.marker.exists(), "the swapped file runs nothing");
}

/// A writer rewriting the file for the whole of many pinned commands. Each
/// command either ran on a clean copy or refused a hostile one. The control
/// shows the same writer reaches git that opens the real path.
#[test]
fn a_writer_racing_every_pinned_command_never_runs_its_filter() {
    let fixture = Fixture::new();
    fixture.make_status_hash();
    let file = fixture.admin.join(CONFIG_WORKTREE);
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let (file, hostile, stop) = (file.clone(), fixture.hostile_config(), stop.clone());
        std::thread::spawn(move || {
            let staged = file.with_extension("staged");
            while !stop.load(Ordering::Relaxed) {
                for content in [SPARSE, hostile.as_str()] {
                    std::fs::write(&staged, content).unwrap();
                    std::fs::rename(&staged, &file).unwrap();
                }
            }
        })
    };

    let (mut ran, mut refused) = (0, 0);
    for _ in 0..40 {
        match fixture.pinned().run(&["status", "--porcelain"]) {
            Ok(output) => {
                assert!(output.status.success(), "{output:?}");
                ran += 1;
            }
            Err(error) => {
                assert!(
                    format!("{error:#}").contains("will not run git"),
                    "{error:#}"
                );
                refused += 1;
            }
        }
        assert!(!fixture.marker.exists(), "a pinned command ran the filter");
    }
    assert_eq!(ran + refused, 40);

    let reached = (0..400).any(|_| {
        fixture.status_with_git_dir(&fixture.admin);
        fixture.marker.exists()
    });
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
    assert!(reached, "the writer never reached git on the real path");
}

/// A session's capsule lists administrative directories when it is built, so
/// a later worktree has no deny and its session-writable file arrives after
/// any check.
#[test]
fn a_worktree_registered_after_the_session_spawned_gets_no_power() {
    let fixture = Fixture::new();
    let capsule = partition_admin_dirs(&fixture.repo, Some(&fixture.worktree)).unwrap();
    ok(
        &fixture.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "loom/late",
            ".worktrees/late",
        ],
    );
    let late = fixture.common.join("worktrees/late");
    assert!(!capsule.siblings.contains(&late), "the capsule covers it");
    let worktree = fixture.repo.join(".worktrees/late");
    ok(&worktree, &["read-tree", "HEAD"]);
    std::fs::write(worktree.join(".gitattributes"), "* filter=x\n").unwrap();
    std::fs::write(worktree.join("README.md"), "changed\n").unwrap();
    let file = late.join(CONFIG_WORKTREE);
    std::fs::write(&file, fixture.hostile_config()).unwrap();
    let pinned = WorktreeGit::pinned(&fixture.repo, &worktree).unwrap();

    let error = pinned.run(&["status", "--porcelain"]).unwrap_err();
    assert!(format!("{error:#}").contains("filter.x.clean"), "{error:#}");
    assert!(!fixture.marker.exists());

    std::fs::remove_file(&file).unwrap();
    let scratch = ScratchAdmin::prepare(&fixture.repo, &late, &fixture.common, true).unwrap();
    std::fs::write(&file, fixture.hostile_config()).unwrap();
    let env = [
        ("GIT_DIR", scratch.path().as_os_str()),
        ("GIT_COMMON_DIR", fixture.common.as_os_str()),
        ("GIT_WORK_TREE", worktree.as_os_str()),
    ];
    let output = run_git_with_env(&["status", "--porcelain"], &env, &worktree).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!fixture.marker.exists(), "git read the late file");
}

/// The positive control runs last: git that follows the planted pointer runs
/// the foreign repository's filter.
#[test]
fn a_planted_foreign_git_directory_configures_nothing() {
    let fixture = Fixture::new();
    let marker = plant_foreign_git_dir(&fixture.worktree);

    let output = fixture.pinned().run(&["status", "--porcelain"]).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(!marker.exists(), "pinned git ran the foreign filter");
    WorktreeGit::discovered(&fixture.worktree)
        .run(&["status", "--porcelain"])
        .unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}

#[test]
fn a_filename_that_folds_to_config_worktree_is_not_linked() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.admin.join("CONFIG.WORKTREE"),
        fixture.hostile_config(),
    )
    .unwrap();

    let scratch = fixture.scratch();

    assert!(!scratch.path().join("CONFIG.WORKTREE").exists());
    assert!(!scratch.path().join(CONFIG_WORKTREE).exists());
}

#[test]
fn the_validated_settings_still_reach_git() {
    let fixture = Fixture::new();
    std::fs::write(fixture.admin.join(CONFIG_WORKTREE), SPARSE).unwrap();

    let value = fixture
        .pinned()
        .run_checked(&["config", "--get", "core.sparseCheckout"])
        .unwrap();

    assert_eq!(value.trim(), "true");
}

#[test]
fn the_private_directory_sits_under_the_denied_state_root_and_is_removed() {
    let fixture = Fixture::new();
    let state = fixture.repo.join(".loom/work").canonicalize().unwrap();
    let scratch = fixture.scratch();
    let path = scratch.path().to_path_buf();
    assert!(path
        .canonicalize()
        .unwrap()
        .starts_with(state.join(PRIVATE_PARENT)));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "the directory is open to others: {mode:o}");

    scratch.publish().unwrap();

    assert!(!path.exists());
}

/// A sandboxed CLI cannot write the state root; its commands still run.
#[test]
fn a_process_that_cannot_write_the_state_root_still_runs_git() {
    let fixture = Fixture::new();
    let state = fixture.repo.join(".loom/work");
    let original = std::fs::metadata(&state).unwrap().permissions();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500)).unwrap();

    let output = fixture.pinned().run(&["status", "--porcelain"]);

    std::fs::set_permissions(&state, original).unwrap();
    assert!(output.unwrap().status.success());
}

#[path = "scratch_admin_publish_tests.rs"]
mod publish;

#[path = "scratch_admin_link_tests.rs"]
mod links;

#[path = "scratch_admin_swap_tests.rs"]
mod swap;

#[path = "scratch_admin_split_index_tests.rs"]
mod split_index;
