//! Contract `base-provision-failure-is-note`: `compare` provisions the base
//! checkout (`BaseRunner::provision`, built on `provision_with`) before any
//! selection runs there. A provision that fails at its first entry with a
//! lockfile mismatch runs no later entry and no selection, and leaves one note
//! with the reason instead of a standing failure. After a provision that
//! succeeds, a real build failure at the base still stands.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tempfile::TempDir;

use super::base::{compare, provision_with, BaseProbe, BaseRunner};
use super::failure::{FailureKind, ImpactFailure};
use super::runs::{Failed, Failure};
use crate::plan::schema::ProvisionEntry;
use crate::test_git::{git, TestRepo};
use crate::testrun::{registry, TestTarget};
use crate::verify::criteria::{ProbeRun, ProbeRunner};

const FULL_SUITE: &str = "the full suite runs in integration-verify";
const LOCKFILE_MISMATCH: &str = "error: lockfile had changes, but lockfile is frozen";
const BUILD_FAILURE: &str = "error[E0425]: cannot find value `x` in this scope\n \
    --> src/lib.rs:1:1\n\nerror: could not compile `demo` (lib test) due to 1 previous error\n";

/// What the base saw, in order: each provision entry's command with the
/// directory it ran in and that directory's `src/lib.rs`, and each selection
/// run's package directory.
#[derive(Debug, PartialEq)]
enum Seen {
    Provision(String, PathBuf, String),
    Run(PathBuf),
}

/// A base whose first provision entry fails with a lockfile mismatch when
/// `mismatch`, and whose selections fail to build.
struct Base {
    mismatch: bool,
    seen: RefCell<Vec<Seen>>,
}

impl BaseRunner for Base {
    fn runner_for(&self, _root: &Path) -> Box<dyn ProbeRunner + '_> {
        Box::new(self)
    }

    fn provision(&self, root: &Path) -> Result<(), String> {
        provision_with(&entries(), root, |entry: &ProvisionEntry, dir: &Path| {
            let lib = fs::read_to_string(dir.join("src/lib.rs")).unwrap_or_default();
            let seen = Seen::Provision(entry.command.clone(), dir.to_path_buf(), lib);
            self.seen.borrow_mut().push(seen);
            if self.mismatch && entry.command.starts_with("bun") {
                Err(LOCKFILE_MISMATCH.to_string())
            } else {
                Ok(())
            }
        })
    }
}

impl ProbeRunner for &Base {
    fn run(&self, _command: &str, package_dir: &Path) -> Result<ProbeRun> {
        self.seen
            .borrow_mut()
            .push(Seen::Run(package_dir.to_path_buf()));
        Ok(ProbeRun {
            stdout: String::new(),
            stderr: BUILD_FAILURE.to_string(),
            exit_code: Some(101),
            timed_out: false,
            output_complete: true,
            cached: None,
        })
    }
}

fn entries() -> Vec<ProvisionEntry> {
    ["bun install --frozen-lockfile", "uv sync --locked"]
        .map(|command| ProvisionEntry {
            working_dir: ".".to_string(),
            command: command.to_string(),
        })
        .to_vec()
}

/// A repository whose `main` holds `src/lib.rs` as `base` under a stage
/// branch that rewrites it; returns it and the base commit.
fn stage_repo() -> (TempDir, String) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    TestRepo::builder().no_seed().init_in(root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "base").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "-q", "-b", "loom/s"]);
    fs::write(root.join("src/lib.rs"), "stage").unwrap();
    git(root, &["commit", "-q", "-am", "stage"]);
    (temp, base)
}

/// The stage's cargo selection, failing as `kind`.
fn failed(kind: FailureKind) -> Failed {
    Failed {
        id: "IT-cargo-test:.".to_string(),
        package: PathBuf::new(),
        adapter: registry::by_name("cargo-test").unwrap(),
        targets: vec![TestTarget {
            file: "src/lib.rs".to_string(),
            name: Some("tests::adds".to_string()),
        }],
        failure: Failure {
            failed_tests: Vec::new(),
            command: "cargo test -- tests::adds".to_string(),
            kind,
            output: BUILD_FAILURE.to_string(),
        },
        accepted: None,
    }
}

/// What one comparison left: the stage repository, the standing failures,
/// the notes and what the base saw.
struct Compared {
    repo: TempDir,
    standing: Vec<ImpactFailure>,
    notes: BTreeSet<String>,
    seen: Vec<Seen>,
}

/// Compare `failure` at the base of a fresh stage repository.
fn compared(mismatch: bool, failure: Failed) -> Compared {
    let (repo, commit) = stage_repo();
    let base = Base {
        mismatch,
        seen: RefCell::new(Vec::new()),
    };
    let probe = BaseProbe {
        worktree: repo.path(),
        commit: Ok(commit),
        runner: &base,
    };
    let mut notes = BTreeSet::new();
    let standing = compare(&probe, vec![failure], &mut notes);
    Compared {
        repo,
        standing,
        notes,
        seen: base.seen.into_inner(),
    }
}

fn a_failed_provision_is_one_note_and_runs_nothing() {
    let kind = FailureKind::Fails { failed: Some(2) };
    let Compared {
        repo,
        standing,
        notes,
        seen,
    } = compared(true, failed(kind));
    assert!(
        standing.is_empty(),
        "a base that cannot be provisioned leaves no standing failure"
    );
    assert_eq!(
        seen.len(),
        1,
        "provision stops at the failed first entry and runs no selection"
    );
    let Seen::Provision(command, root, lib) = &seen[0] else {
        panic!("the base is provisioned before anything runs there");
    };
    assert_eq!(
        command, "bun install --frozen-lockfile",
        "the first entry runs first"
    );
    assert!(
        !root.starts_with(repo.path()),
        "provision runs in a checkout of its own"
    );
    assert_eq!(
        lib, "base",
        "provision runs in the checkout of the base commit"
    );
    let note = notes.first().map(String::as_str).unwrap_or_default();
    assert!(
        notes.len() == 1 && note.contains("lockfile had changes") && note.contains(FULL_SUITE),
        "one note gives the provision failure and the full-suite hint"
    );
}

fn a_real_build_failure_after_provision_stands() {
    let Compared {
        standing,
        notes,
        seen,
        ..
    } = compared(false, failed(FailureKind::DoesNotBuild));
    assert_eq!(
        standing.len(),
        1,
        "a build failure at a provisioned base stands"
    );
    assert!(notes.is_empty(), "a standing build failure leaves no note");
    let [Seen::Provision(first, root, _), Seen::Provision(second, again, _), Seen::Run(ran)] =
        seen.as_slice()
    else {
        panic!("both entries provision the base, then the selection runs there");
    };
    assert!(
        first.starts_with("bun") && second.starts_with("uv"),
        "entries run in order"
    );
    assert!(
        root == again && root == ran,
        "provision and selection share one checkout"
    );
}

#[test]
fn a_base_whose_provision_fails_is_noted_not_failed() {
    a_failed_provision_is_one_note_and_runs_nothing();
    a_real_build_failure_after_provision_stands();
}
