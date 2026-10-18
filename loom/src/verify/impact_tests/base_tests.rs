//! Re-running failed selections in a real checkout of the stage's base, with
//! canned runner output standing in for the test runner.

use super::*;
use crate::testrun::{registry, TestTarget};
use crate::verify::criteria::ProbeRun;
use crate::verify::impact_tests::failure::FailureKind;
use crate::verify::impact_tests::runs::Failure;
use crate::verify::impact_tests::tests::git;
use std::cell::RefCell;
use std::fs;
use tempfile::TempDir;

/// Cargo's summary for a run with `failed` of 3 tests failing.
fn cargo_run(failed: u32) -> ProbeRun {
    let passed = 3 - failed;
    let verdict = if failed == 0 { "ok" } else { "FAILED" };
    ProbeRun {
        stdout: format!(
            "running 3 tests\n\ntest result: {verdict}. {passed} passed; {failed} failed; 0 \
             ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
        ),
        stderr: String::new(),
        exit_code: Some(if failed == 0 { 0 } else { 101 }),
        timed_out: false,
    }
}

/// Answers every run with `answer`, recording each package directory and the
/// `src/lib.rs` it found there.
struct Canned {
    answer: fn() -> Result<ProbeRun>,
    seen: RefCell<Vec<(PathBuf, String)>>,
}

impl Canned {
    fn new(answer: fn() -> Result<ProbeRun>) -> Self {
        Self {
            answer,
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl BaseRunner for Canned {
    fn runner_for(&self, _root: &Path) -> Box<dyn ProbeRunner + '_> {
        Box::new(self)
    }
}

impl ProbeRunner for &Canned {
    fn run(&self, _command: &str, package_dir: &Path) -> Result<ProbeRun> {
        let lib = fs::read_to_string(package_dir.join("src/lib.rs")).unwrap_or_default();
        self.seen
            .borrow_mut()
            .push((package_dir.to_path_buf(), lib));
        (self.answer)()
    }
}

/// A repository whose `main` holds `src/lib.rs` as `base`, and a stage branch
/// checked out on top that rewrites it as `stage`; returns it and the base.
fn stage_repo() -> (TempDir, String) {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
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

/// The cargo selection `IT-cargo-test:.` failing with `failed` failures.
fn failed(failed: u64) -> Failed {
    Failed {
        id: "IT-cargo-test:.".to_string(),
        package: PathBuf::new(),
        adapter: registry::by_name("cargo-test").unwrap(),
        targets: vec![TestTarget {
            file: "src/lib.rs".to_string(),
            name: Some("tests::adds".to_string()),
        }],
        failure: Failure {
            command: "cargo test -- tests::adds".to_string(),
            kind: FailureKind::Fails {
                failed: Some(failed),
            },
            output: "test result: FAILED".to_string(),
        },
        accepted: None,
    }
}

/// Compare `failure` at `commit` of the repository at `worktree`, every base
/// run answered by `canned`.
fn compare_with(
    worktree: &Path,
    commit: Result<String, String>,
    canned: &Canned,
    failure: Failed,
) -> (Vec<ImpactFailure>, BTreeSet<String>) {
    let base = BaseProbe {
        worktree,
        commit,
        runner: canned,
    };
    let mut notes = BTreeSet::new();
    let standing = compare(&base, vec![failure], &mut notes);
    (standing, notes)
}

#[test]
fn a_selection_failing_as_badly_at_the_base_is_a_note() {
    let (repo, base) = stage_repo();
    let canned = Canned::new(|| Ok(cargo_run(2)));

    let (standing, notes) = compare_with(repo.path(), Ok(base.clone()), &canned, failed(2));

    assert!(standing.is_empty(), "{standing:?}");
    let note = notes.iter().next().unwrap();
    assert!(
        note.starts_with("`IT-cargo-test:.` fails (2 failed) at the stage's base"),
        "{note}"
    );
    assert!(note.contains(&base[..SHORT_COMMIT]), "{note}");
    // The run saw the base's tree, not the stage's.
    assert_eq!(canned.seen.borrow()[0].1, "base");
}

#[test]
fn a_selection_passing_at_the_base_stays_a_failure() {
    let (repo, base) = stage_repo();
    let canned = Canned::new(|| Ok(cargo_run(0)));

    let (standing, notes) = compare_with(repo.path(), Ok(base), &canned, failed(1));

    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(standing.len(), 1);
    assert_eq!(standing[0].id, "IT-cargo-test:.");
    assert!(standing[0]
        .base
        .starts_with("it passes at the stage's base"));
    assert_eq!(standing[0].tests, vec!["tests::adds".to_string()]);
}

#[test]
fn more_failures_than_at_the_base_stay_a_failure() {
    let (repo, base) = stage_repo();
    let canned = Canned::new(|| Ok(cargo_run(1)));

    let (standing, _) = compare_with(repo.path(), Ok(base), &canned, failed(2));

    assert_eq!(standing.len(), 1);
    assert!(
        standing[0].base.ends_with("it fails (1 failed) instead"),
        "{}",
        standing[0].base
    );
}

/// The base checkout often cannot build for reasons of its own (tracked files
/// only), so a selection that does not build at both stays the stage's
/// failure: it could be a compile error the stage made.
#[test]
fn a_selection_that_does_not_build_at_the_base_either_stays_a_failure() {
    let (repo, base) = stage_repo();
    let canned = Canned::new(|| {
        Ok(ProbeRun {
            stdout: String::new(),
            stderr: include_str!("../../testrun/fixtures/cargo-test/build-error.stderr")
                .to_string(),
            exit_code: Some(101),
            timed_out: false,
        })
    });
    let mut broken = failed(0);
    broken.failure.kind = FailureKind::DoesNotBuild;

    let (standing, notes) = compare_with(repo.path(), Ok(base), &canned, broken);

    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(standing.len(), 1);
    assert_eq!(standing[0].kind, FailureKind::DoesNotBuild);
    assert!(
        standing[0]
            .base
            .starts_with("it does not build at the stage's base"),
        "{}",
        standing[0].base
    );
}

/// Snapshot of every path under `dir`, recursively.
fn tree(dir: &Path) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap().flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(path.clone());
            }
            paths.insert(path);
        }
    }
    paths
}

/// A stage worktree is a linked worktree, whose git directory differs from
/// the repository's common one. The base checkout clones the common
/// directory and writes nothing into it: no worktree registration, lock or
/// object lands in the main repository's `.git`.
#[test]
fn a_base_run_from_a_linked_worktree_adds_nothing_to_the_main_git_directory() {
    let (repo, base) = stage_repo();
    git(repo.path(), &["checkout", "-q", "main"]);
    let linked = repo.path().join(".worktrees").join("s");
    git(
        repo.path(),
        &["worktree", "add", "-q", linked.to_str().unwrap(), "loom/s"],
    );
    let git_dir = git(&linked, &["rev-parse", "--absolute-git-dir"]);
    let common = git(
        &linked,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    assert_ne!(git_dir, common, "the fixture is not a linked worktree");
    let before = tree(&repo.path().join(".git"));
    let canned = Canned::new(|| Ok(cargo_run(2)));

    let (standing, notes) = compare_with(&linked, Ok(base), &canned, failed(2));

    assert!(standing.is_empty(), "{standing:?}");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(canned.seen.borrow()[0].1, "base");
    let added: Vec<_> = tree(&repo.path().join(".git"))
        .difference(&before)
        .cloned()
        .collect();
    assert!(added.is_empty(), "the base run added {added:?}");
}

/// The checkout is gone once the comparison returns, also when the base run
/// itself errors.
#[test]
fn the_base_checkout_is_removed_after_a_failed_run() {
    let (repo, base) = stage_repo();
    let canned = Canned::new(|| Err(anyhow::anyhow!("spawn refused")));

    let (standing, _) = compare_with(repo.path(), Ok(base), &canned, failed(1));

    assert_eq!(standing.len(), 1);
    assert!(
        standing[0].base.contains("proved nothing"),
        "{}",
        standing[0].base
    );
    assert!(
        standing[0].base.contains("spawn refused"),
        "{}",
        standing[0].base
    );
    let (checkout, lib) = canned.seen.borrow()[0].clone();
    assert_eq!(lib, "base");
    assert!(!checkout.exists(), "{} was left behind", checkout.display());
    assert!(!checkout.parent().unwrap().exists());
}

#[test]
fn a_base_that_cannot_be_compared_keeps_the_failure_with_why() {
    let (repo, _) = stage_repo();
    let canned = Canned::new(|| Ok(cargo_run(1)));

    let unknown = Err("no merge base".to_string());
    let (unknown, _) = compare_with(repo.path(), unknown, &canned, failed(1));
    let missing = Ok("0".repeat(40));
    let (missing, _) = compare_with(repo.path(), missing, &canned, failed(1));

    assert!(
        unknown[0].base.contains("base is unknown (no merge base)"),
        "{}",
        unknown[0].base
    );
    assert!(
        missing[0].base.contains("could not be checked out"),
        "{}",
        missing[0].base
    );
    assert!(canned.seen.borrow().is_empty());
}
