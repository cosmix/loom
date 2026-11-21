//! Reconstructing a disputed selection's command from the checkout: shown only
//! for the selection the failure reported.

use super::*;
use crate::models::stage::Stage;
use crate::testrun::TestTarget;
use crate::verify::impact_tests::failure::failure_id;
use crate::verify::impact_tests::FailureKind;

const RUNNER: &str = "cargo-test";

fn target(name: &str) -> TestTarget {
    TestTarget {
        file: "src/lib.rs".to_string(),
        name: Some(name.to_string()),
    }
}

fn selected(count: usize) -> Vec<TestTarget> {
    (0..count)
        .map(|n| target(&format!("suite::case_{n:03}")))
        .collect()
}

/// The failure of `package`'s selection of `targets`, as a run records it.
fn failure(package: &str, targets: &[TestTarget]) -> ImpactFailure {
    ImpactFailure {
        id: failure_id(RUNNER, Path::new(package)),
        command: "cargo test -- suite::case_000 … (721 targets)".to_string(),
        tests: Vec::new(),
        selection_digest: selection_digest(targets),
        kind: FailureKind::Fails { failed: Some(1) },
        base: "it passes at the stage's base".to_string(),
        accepted: None,
        output: String::new(),
    }
}

fn plan_of(root: &Path, package: &str, targets: Vec<TestTarget>) -> ReplayPlan {
    ReplayPlan::of_selection(root, RUNNER, Path::new(package), targets)
}

#[test]
fn a_selection_of_721_tests_is_replayed_with_every_test() {
    let root = tempfile::tempdir().unwrap();
    let targets = selected(721);
    let plan = plan_of(root.path(), "", targets.clone());

    let replay = plan.replay(&failure("", &targets)).unwrap();

    assert_eq!(replay.directory, root.path());
    for target in &targets {
        let name = target.name.as_deref().unwrap();
        assert!(replay.command.contains(name), "{name} is missing");
    }
}

#[test]
fn the_order_and_repeats_of_the_targets_do_not_matter() {
    let root = tempfile::tempdir().unwrap();
    let mut targets = selected(5);
    let failure = failure("", &targets);
    targets.reverse();
    targets.push(target("suite::case_002"));

    let replay = plan_of(root.path(), "", targets).replay(&failure);

    assert!(replay.is_ok(), "{replay:?}");
}

#[test]
fn a_package_directory_is_the_checkout_joined_with_the_package() {
    let root = tempfile::tempdir().unwrap();
    let targets = selected(2);
    let plan = plan_of(root.path(), "crates/app", targets.clone());

    let replay = plan.replay(&failure("crates/app", &targets)).unwrap();

    assert_eq!(replay.directory, root.path().join("crates/app"));
}

#[test]
fn a_changed_selection_is_no_replay() {
    let root = tempfile::tempdir().unwrap();
    let targets = selected(721);
    let reported = failure("", &targets);
    let mut changed = targets;
    changed[700] = target("suite::renamed");

    let error = plan_of(root.path(), "", changed)
        .replay(&reported)
        .unwrap_err();

    assert!(error.to_string().contains("selects other tests"), "{error}");
}

#[test]
fn another_runner_or_package_or_a_missing_digest_is_no_replay() {
    let root = tempfile::tempdir().unwrap();
    let targets = selected(3);
    let plan = plan_of(root.path(), "", targets.clone());
    let mut no_digest = failure("", &targets);
    no_digest.selection_digest = String::new();
    let mut other_runner = failure("", &targets);
    other_runner.id = failure_id("vitest", Path::new(""));
    let mut malformed = failure("", &targets);
    malformed.id = "not-an-id".to_string();

    for (failure, why) in [
        (failure("web", &targets), "no cargo-test tests in `web`"),
        (other_runner, "no vitest tests in `.`"),
        (no_digest, "no selection digest"),
        (malformed, "not an impact-test id"),
    ] {
        let error = plan.replay(&failure).unwrap_err();
        assert!(error.to_string().contains(why), "{why}: {error}");
    }
}

const CARGO_TOML: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LIB: &str = "pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n\n\
                   #[cfg(test)]\nmod add_tests;\n";
const ADD_TESTS: &str = "use crate::add;\n\n#[test]\nfn adds_two() {\n    \
                         let sum = add(1, 2);\n    assert_eq!(sum, 3);\n}\n";
const MORE_TESTS: &str = "use crate::add;\n\n#[test]\nfn adds_more() {\n    \
                          let sum = add(2, 2);\n    assert_eq!(sum, 4);\n}\n";

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A crate on `main` whose `add` one test calls, and a stage branch that
/// changes `add`; returns the checkout and the stage's base commit.
fn stage_repo() -> (tempfile::TempDir, PathBuf, String) {
    use crate::test_git::{git, TestRepo};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    TestRepo::builder().no_seed().init_in(&root);
    write(&root, "Cargo.toml", CARGO_TOML);
    write(&root, "src/lib.rs", LIB);
    write(&root, "src/add_tests.rs", ADD_TESTS);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "seed"]);
    let work = root.join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    crate::git::worktree::capture_main_identity(&root, &work).unwrap();
    let base = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["checkout", "-b", "loom/s"]);
    write(&root, "src/lib.rs", &LIB.replace("a + b", "b + a"));
    git(&root, &["commit", "-am", "the stage's change"]);
    (temp, root, base)
}

/// Built from a real checkout, the plan replays the selection a run reported;
/// once the checkout selects another test, the same evidence is no replay.
#[test]
fn a_replay_follows_the_checkout_the_selection_was_derived_from() {
    use crate::test_git::git;
    let (_temp, root, base) = stage_repo();
    let stage = Stage {
        id: "s".to_string(),
        ..Stage::default()
    };
    let plan = ReplayPlan::build(&stage, &root, &base).unwrap();
    let (_, targets) = plan.plan.groups.values().next().expect("one selection");
    let reported = failure("", targets);

    let replay = plan.replay(&reported).unwrap();

    assert!(replay.command.contains("add_tests::adds_two"), "{replay:?}");
    assert_eq!(replay.directory, root);

    write(
        &root,
        "src/lib.rs",
        &format!("{LIB}#[cfg(test)]\nmod more_tests;\n"),
    );
    write(&root, "src/more_tests.rs", MORE_TESTS);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "another test"]);
    let later = ReplayPlan::build(&stage, &root, &base).unwrap();

    let error = later.replay(&reported).unwrap_err();
    assert!(error.to_string().contains("selects other tests"), "{error}");
}
