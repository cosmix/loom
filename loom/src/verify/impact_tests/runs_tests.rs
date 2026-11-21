//! Selections that cannot be run, and how a long selection displays.

use super::*;
use crate::models::stage::Stage;
use crate::testrun::registry;
use crate::verify::criteria::{CachedRun, CriteriaConfig, CriteriaProbe};
use anyhow::Result;
use tempfile::TempDir;

fn cargo() -> &'static dyn TestRunnerAdapter {
    registry::by_name("cargo-test").unwrap()
}

fn named(names: impl IntoIterator<Item = String>) -> Vec<TestTarget> {
    names
        .into_iter()
        .map(|name| TestTarget {
            file: "src/lib.rs".to_string(),
            name: Some(name),
        })
        .collect()
}

/// Fails as the real runner does when exec refuses the command: an OS error
/// under a context that repeats the whole command.
struct CannotSpawn;

impl ProbeRunner for CannotSpawn {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        let os_error = anyhow::Error::from(std::io::Error::from_raw_os_error(7));
        Err(os_error.context(format!(
            "Failed to execute criterion: {}",
            "x".repeat(300_000)
        )))
    }
}

#[test]
fn a_selection_that_cannot_start_is_a_note() {
    let root = TempDir::new().unwrap();
    let mut selection = Selection::new(root.path(), &CannotSpawn, BTreeSet::new());
    let targets = named(["a", "b", "c"].map(String::from));
    selection.run_group(Path::new(""), cargo(), &targets);
    assert!(selection.ran.is_empty());
    assert!(selection.failed.is_empty());
    assert_eq!(selection.notes.len(), 1);
    let note = selection.notes.first().unwrap();
    assert!(note.contains("could not be run"), "{note}");
    assert!(note.contains(FULL_SUITE), "{note}");
    assert!(note.len() < 1_000, "{} bytes", note.len());
}

#[test]
fn an_oversized_selection_fails_exec_and_becomes_a_note() {
    if !crate::process::sandbox_probe::confined_commands_runnable() {
        return;
    }
    let root = TempDir::new().unwrap();
    let stage = Stage::default();
    let probe = CriteriaProbe {
        stage: &stage,
        config: CriteriaConfig::default(),
    };
    let mut selection = Selection::new(root.path(), &probe, BTreeSet::new());
    let targets = named(["a".repeat(4 << 20)]);
    selection.run_group(Path::new(""), cargo(), &targets);
    assert!(selection.ran.is_empty());
    assert_eq!(selection.notes.len(), 1);
    let note = selection.notes.first().unwrap();
    assert!(
        note.contains("could not be run"),
        "{}",
        &note[..200.min(note.len())]
    );
    assert!(note.len() < 1_000, "{} bytes", note.len());
}

/// A selection runs in its package with the whole checkout visible, so the
/// package's sibling packages resolve as they do for acceptance at the root.
#[test]
fn a_probe_with_a_tree_root_reads_sibling_packages() {
    if !crate::process::sandbox_probe::confined_commands_runnable() {
        return;
    }
    let root = TempDir::new().unwrap();
    let package = root.path().join("pkg");
    std::fs::create_dir(&package).unwrap();
    std::fs::create_dir(root.path().join("contracts")).unwrap();
    std::fs::write(root.path().join("contracts/schema"), "shared").unwrap();
    let stage = Stage::default();
    let probe = CriteriaProbe {
        stage: &stage,
        config: CriteriaConfig::default().with_tree_root(Some(root.path())),
    };
    let run = probe.run("cat ../contracts/schema", &package).unwrap();
    assert_eq!(run.exit_code, Some(0), "{}", run.stderr);
    assert_eq!(run.stdout, "shared");
}

#[test]
fn a_short_command_displays_unchanged() {
    assert_eq!(shown_command("cargo test -- a", 1), "cargo test -- a");
}

#[test]
fn a_long_command_is_shortened_with_its_target_count() {
    let command = format!("cargo test -- {}", "é".repeat(500));
    let shown = shown_command(&command, 500);
    assert!(shown.ends_with("… (500 targets)"), "{shown}");
    assert!(shown.len() < COMMAND_DISPLAY_CHARS + 30);
    assert!(shown.starts_with("cargo test -- é"));
}

#[test]
fn selected_lists_the_first_ten_names() {
    use super::super::failure::listed;
    let tests = labels(&named((0..25).map(|n| format!("t{n}"))));
    let shown = listed(&tests);
    assert!(shown.starts_with("selected: t0, t1,"), "{shown}");
    assert!(shown.contains("t9, and 15 more"), "{shown}");
    assert!(!shown.contains("t10"), "{shown}");
    assert_eq!(listed(&tests[..3]), "selected: t0, t1, t2");
}

/// `uv run` cannot build a missing `.venv` with the network denied, so a uv
/// package without one is a note and never runs; a workspace `.venv` in an
/// ancestor counts.
#[test]
fn a_uv_package_without_a_venv_is_a_note() {
    let root = TempDir::new().unwrap();
    let package = Path::new("deps/kit");
    std::fs::create_dir_all(root.path().join(package)).unwrap();
    std::fs::write(root.path().join(package).join("uv.lock"), "").unwrap();
    let pytest = registry::by_name("pytest").unwrap();
    let targets = [TestTarget {
        file: "tests/test_kit.py".to_string(),
        name: None,
    }];
    let notes_after_run = || {
        let mut selection = Selection::new(root.path(), &CannotSpawn, BTreeSet::new());
        selection.run_group(package, pytest, &targets);
        selection.notes.into_iter().collect::<Vec<_>>()
    };

    let notes = notes_after_run();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].starts_with("`deps/kit` has no .venv"), "{notes:?}");

    std::fs::create_dir(root.path().join(".venv")).unwrap();
    let notes = notes_after_run();
    assert!(notes[0].contains("could not be run"), "{notes:?}");
}

/// The kept tail of a certified cache hit: only trailing `running 0 tests`
/// blocks of other test binaries, with the exit code of a passing run.
fn cached_zero_test_tail(tests_executed: Option<u64>) -> ProbeRun {
    const ZERO_TEST_BLOCK: &str = "running 0 tests\n\n\
        test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n";
    ProbeRun {
        stdout: ZERO_TEST_BLOCK.repeat(2),
        stderr: String::new(),
        exit_code: Some(0),
        timed_out: false,
        output_complete: true,
        cached: Some(CachedRun { tests_executed }),
    }
}

#[test]
fn a_cached_pass_is_judged_by_its_recorded_count() {
    let run = cached_zero_test_tail(Some(3));
    let verdict = judge("cargo test x".to_string(), cargo(), &run);
    assert!(matches!(verdict, Run::Passed { note: None, .. }));
}

#[test]
fn incomplete_capture_is_inconclusive_even_with_a_pass_or_cached_count() {
    for cached in [
        None,
        Some(CachedRun {
            tests_executed: Some(3),
        }),
    ] {
        let mut run = cached_zero_test_tail(Some(3));
        run.cached = cached;
        run.output_complete = false;
        let Run::Inconclusive(why) = judge("cargo test x".to_string(), cargo(), &run) else {
            panic!("incomplete output must be inconclusive");
        };
        let why = why.to_string();
        assert!(why.contains("output capture was incomplete"), "{why}");
    }
}

#[test]
fn a_cached_run_that_selected_nothing_stays_inconclusive() {
    let run = cached_zero_test_tail(Some(0));
    let Run::Inconclusive(why) = judge("cargo test x".to_string(), cargo(), &run) else {
        panic!("a cached run that selected no test must be inconclusive");
    };
    let why = why.to_string();
    assert!(why.contains("selected no test"), "{why}");
}

/// A run with an over-long log and a failing exit is a failure: the capture
/// limit says nothing about the exit code, and a note would let the gate pass.
#[test]
fn incomplete_capture_of_a_failing_run_stays_failed_and_unparsed() {
    for exit_code in [Some(1), Some(101), None] {
        let mut run = cached_zero_test_tail(None);
        run.cached = None;
        run.output_complete = false;
        run.exit_code = exit_code;
        let Run::Failed(failure) = judge("cargo test x".to_string(), cargo(), &run) else {
            panic!("exit {exit_code:?} with incomplete output must fail");
        };
        assert_eq!(failure.kind, FailureKind::Unparsed { exit_code });
    }
}

#[test]
fn incomplete_capture_of_a_run_that_could_not_start_stays_inconclusive() {
    let mut run = cached_zero_test_tail(None);
    run.cached = None;
    run.output_complete = false;
    run.exit_code = Some(127);
    let Run::Inconclusive(why) = judge("cargo test x".to_string(), cargo(), &run) else {
        panic!("exit 127 must stay inconclusive");
    };
    let why = why.to_string();
    assert!(why.contains("could not start"), "{why}");
}

/// A finished, completely captured run printing `stdout` and `stderr`.
fn finished(stdout: &str, stderr: &str, exit_code: Option<i32>) -> ProbeRun {
    ProbeRun {
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        exit_code,
        timed_out: false,
        output_complete: true,
        cached: None,
    }
}

/// The failed tests are read from the whole output, so a name printed before
/// the recorded tail still reaches the gate.
#[test]
fn a_failed_run_names_its_failed_tests_beyond_the_recorded_tail() {
    let stdout = format!(
        "running 2 tests\ntest a::early ... FAILED\n{}\ntest a::late ... ok\n\n\
         test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured\n",
        "noise\n".repeat(2_000)
    );
    let run = finished(&stdout, "", Some(101));
    let Run::Failed(failure) = judge("cargo test a".to_string(), cargo(), &run) else {
        panic!("a counted failure must fail");
    };
    assert!(!failure.output.contains("a::early"), "the tail is cut");
    assert_eq!(failure.failed_tests, vec!["a::early".to_string()]);
}

#[test]
fn a_failed_unparsed_run_with_a_denial_is_an_environment_inconclusive() {
    let denied = finished(
        "",
        "sandbox-exec: sandbox_apply: Operation not permitted\n",
        Some(71),
    );
    let Run::Inconclusive(why) = judge("cargo test x".to_string(), cargo(), &denied) else {
        panic!("a sandbox refusal must be inconclusive");
    };
    assert!(why.is_environment());
    assert_eq!(
        why.to_string(),
        "environment failure (sandbox denial: sandbox-exec: sandbox_apply: Operation not permitted)"
    );
    let other = Inconclusive::from("`x` selected no test".to_string());
    assert!(!other.is_environment());
}

#[test]
fn a_missing_dependency_is_an_environment_inconclusive() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("deps/kit")).unwrap();
    std::fs::write(root.path().join("deps/kit/uv.lock"), "").unwrap();
    let site = Site {
        root: root.path(),
        runner: &CannotSpawn,
        place: "this checkout",
    };
    let pytest = registry::by_name("pytest").unwrap();
    let Run::Inconclusive(why) =
        site.run_selection(Path::new("deps/kit"), pytest, &named(["t".to_string()]))
    else {
        panic!("a package without its environment must be inconclusive");
    };
    assert!(why.is_environment());
    assert!(why.to_string().starts_with("`deps/kit` has no .venv"));
    let ran = site.run_selection(Path::new(""), cargo(), &named(["t".to_string()]));
    assert!(matches!(ran, Run::Inconclusive(why) if !why.is_environment()));
}

#[test]
fn a_denied_selection_notes_the_full_suite_hint_once() {
    struct Denied;
    impl ProbeRunner for Denied {
        fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
            Ok(finished(
                "",
                "bwrap: Creating new namespace failed\n",
                Some(1),
            ))
        }
    }
    let root = TempDir::new().unwrap();
    let mut selection = Selection::new(root.path(), &Denied, BTreeSet::new());
    selection.run_group(Path::new(""), cargo(), &named(["a".to_string()]));
    assert!(selection.failed.is_empty() && selection.ran.is_empty());
    let notes: Vec<_> = selection.notes.iter().collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].starts_with("environment failure (sandbox denial: bwrap:"));
    assert_eq!(notes[0].matches(FULL_SUITE).count(), 1, "{notes:?}");
}

/// Output carrying the marker words keeps its own verdict when a test summary,
/// a build failure, an incomplete capture or a passing exit decides.
#[test]
fn a_runs_own_verdict_wins_over_marker_words_in_its_output() {
    let counted = "running 1 test\ntest t::k ... FAILED\n\nfailures:\n\n---- t::k stdout ----\n\
        assertion failed: kill returned EPERM\n\nfailures:\n    t::k\n\n\
        test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
    let run = finished(counted, "", Some(101));
    let Run::Failed(failure) = judge("cargo test x".to_string(), cargo(), &run) else {
        panic!("a counted failure must stay a failure");
    };
    assert_eq!(failure.kind, FailureKind::Fails { failed: Some(1) });
    let build = "error[E0425]: cannot find value `EPERM` in this scope\n\
        error: could not compile `demo` (lib test) due to 1 previous error\n";
    let run = finished("", build, Some(101));
    let Run::Failed(failure) = judge("cargo test x".to_string(), cargo(), &run) else {
        panic!("a build failure must stay a failure");
    };
    assert_eq!(failure.kind, FailureKind::DoesNotBuild);
    let mut run = finished("", "Error: kill EPERM\n", Some(1));
    run.output_complete = false;
    assert!(matches!(
        judge("cargo test x".to_string(), cargo(), &run),
        Run::Failed(_)
    ));
    let run = finished("", "Error: kill EPERM\n", Some(0));
    assert!(matches!(
        judge("cargo test x".to_string(), cargo(), &run),
        Run::Passed { .. }
    ));
}

#[test]
fn two_selections_denied_alike_keep_a_note_naming_each_command() {
    struct Denied(std::cell::RefCell<Vec<String>>);
    impl ProbeRunner for Denied {
        fn run(&self, command: &str, _package_dir: &Path) -> Result<ProbeRun> {
            self.0.borrow_mut().push(command.to_string());
            Ok(finished(
                "",
                "bwrap: Creating new namespace failed\n",
                Some(1),
            ))
        }
    }
    let root = TempDir::new().unwrap();
    let denied = Denied(std::cell::RefCell::new(Vec::new()));
    let mut selection = Selection::new(root.path(), &denied, BTreeSet::new());
    selection.run_group(Path::new(""), cargo(), &named(["a".to_string()]));
    selection.run_group(Path::new(""), cargo(), &named(["b".to_string()]));
    let notes: Vec<_> = selection.notes.iter().collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
    for command in denied.0.borrow().iter() {
        let running = format!("running `{command}`; {FULL_SUITE}");
        assert!(
            notes.iter().any(|note| note.ends_with(&running)),
            "no note names {command}: {notes:?}"
        );
    }
}
