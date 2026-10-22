//! Detects an unfiltered run of any test runner loom has an adapter for and
//! warns when it appears outside the integration-verify stage, and warns in
//! integration-verify when a filtered run repeats work a full-suite criterion
//! already does. Split out of `validation.rs` (same reason as
//! `structural_checks.rs`): a standard stage's acceptance should prove the
//! code that stage wrote, not re-run the whole suite - the unfiltered run
//! belongs to integration-verify alone, per `skills/loom-plan-writer/SKILL.md`
//! ("The full suite runs once, in integration-verify").

use std::path::Path;

use super::types::{StageDefinition, StageType};
use crate::testrun::recognize::{has_flag, invocations, split_double_dash, strip_prefixes};
use crate::testrun::{registry, TestRunnerAdapter};

/// Flags that select tests the adapter's full run does not execute, so a
/// filtered run using them proves something the full-suite criterion does
/// not: `--all-targets` runs no doctests, `--ignored` and `--include-ignored`
/// run tests the default run skips, pytest `-m`/`--markexpr` can select
/// markers the default run deselects.
const NOT_COVERED_FLAGS: &[&str] = &[
    "--doc",
    "--ignored",
    "--include-ignored",
    "-m",
    "--markexpr",
];

/// One test-runner invocation found in an acceptance command.
struct Run {
    adapter: &'static dyn TestRunnerAdapter,
    argv: Vec<String>,
    full: bool,
}

/// The first test-runner invocation of `command`, or `None` when no adapter
/// recognises one or it only compiles (`--no-run`). Package scoping (`-p`)
/// narrows a Rust run to less than the workspace even though the adapter
/// counts it as full, so it is treated as filtered here.
fn classify(command: &str) -> Option<Run> {
    // `.` as cwd: package-script indirection (`npm test`) reads the
    // `package.json` next to where the lint runs, which is the plan's project.
    let (adapter, argv) = invocations(command, Path::new("."))
        .into_iter()
        .find_map(|argv| {
            let adapter = registry::all()
                .iter()
                .copied()
                .find(|a| a.recognizes(&argv))?;
            Some((adapter, argv))
        })?;
    if has_flag(&argv, "--no-run") {
        return None; // compiles (or lists) only; never executes a test
    }
    let stripped = strip_prefixes(&argv);
    let (options, _) = split_double_dash(&stripped);
    let package_scoped = adapter.language() == "rust" && scopes_package(options);
    let full = adapter.is_full_run(&without_skips(&argv)) && !package_scoped;
    Some(Run {
        adapter,
        argv,
        full,
    })
}

/// Whether cargo's `options` scope the run to a package: `-p foo`, `-pfoo`,
/// `-p=foo` or `--package foo`.
fn scopes_package(options: &[String]) -> bool {
    let attached = |word: &String| word.starts_with("-p") && !word.starts_with("--");
    has_flag(options, "--package") || options.iter().any(attached)
}

/// `argv` without `--skip <name>` pairs: excluding a handful of tests still
/// runs the rest of the suite, so a run that only skips stays a full run.
fn without_skips(argv: &[String]) -> Vec<String> {
    let mut kept = Vec::with_capacity(argv.len());
    let mut words = argv.iter();
    while let Some(word) = words.next() {
        if word == "--skip" {
            words.next();
        } else {
            kept.push(word.clone());
        }
    }
    kept
}

/// True when `command` is an unfiltered run of a recognised test runner -
/// the whole suite, not a package/target/name-filtered slice of it.
#[cfg(test)]
pub(super) fn is_full_suite_run(command: &str) -> bool {
    classify(command).is_some_and(|run| run.full)
}

/// The leading `VAR=value` words of `argv`, which change what the run does.
fn environment(argv: &[String]) -> Vec<&String> {
    argv.iter()
        .take_while(|word| word.as_str() == "env" || word.contains('='))
        .filter(|word| word.as_str() != "env")
        .collect()
}

/// Cargo flags that change what gets built or which crates run, so two runs
/// that differ in them cover different code.
const BUILD_CONFIG_FLAGS: &[&str] = &[
    "--features",
    "-F",
    "--all-features",
    "--no-default-features",
    "--release",
    "--profile",
    "--target",
    "--workspace",
    "--all",
    "-p",
    "--package",
    "--manifest-path",
];

/// Sorted `flag=value` pairs of the build-configuration flags in `argv`, in
/// `--flag value`, `--flag=value` and attached short (`-Ftls`, `-pfoo`) forms
/// (a bare switch has an empty value). Words after a bare `--` belong to the
/// test binary and are ignored.
fn build_config(argv: &[String]) -> Vec<String> {
    let stripped = strip_prefixes(argv);
    let (options, _) = split_double_dash(&stripped);
    let mut found = Vec::new();
    for (i, word) in options.iter().enumerate() {
        let (name, inline) = match word.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => match (word.get(..2), word.get(2..)) {
                (Some(short), Some(rest)) if !word.starts_with("--") && !rest.is_empty() => {
                    (short, Some(rest))
                }
                _ => (word.as_str(), None),
            },
        };
        if !BUILD_CONFIG_FLAGS.contains(&name) {
            continue;
        }
        let value = inline
            .or_else(|| {
                options
                    .get(i + 1)
                    .map(String::as_str)
                    .filter(|next| !next.starts_with('-'))
            })
            .unwrap_or_default();
        found.push(format!("{name}={value}"));
    }
    found.sort();
    found
}

/// Cargo flags that build targets a plain `cargo test` leaves out.
const UNBUILT_TARGET_FLAGS: &[&str] = &["--bench", "--benches", "--example", "--examples"];

/// The words of `argv` ahead of a bare `--`, runner prefixes stripped: the
/// run's own command and options, not the test binary's.
fn run_words(argv: &[String]) -> Vec<String> {
    let stripped = strip_prefixes(argv);
    split_double_dash(&stripped).0.to_vec()
}

/// The stripped `argv` with `extra` added to the run's own options, ahead of
/// a bare `--`.
fn with_options(argv: &[String], extra: &[&str]) -> Vec<String> {
    let mut words = strip_prefixes(argv);
    let at = words.iter().position(|w| w == "--").unwrap_or(words.len());
    let tail = words.split_off(at);
    words.extend(extra.iter().map(|word| word.to_string()));
    words.extend(tail);
    words
}

/// A filtered cargo run that names benches or examples is covered only by a
/// full run that builds them: `--all-targets` or the same flag.
fn unbuilt_targets_covered(full: &Run, filtered: &Run) -> bool {
    if filtered.adapter.language() != "rust" {
        return true;
    }
    let (full_words, words) = (run_words(&full.argv), run_words(&filtered.argv));
    let built = has_flag(&full_words, "--all-targets");
    UNBUILT_TARGET_FLAGS
        .iter()
        .filter(|flag| has_flag(&words, flag))
        .all(|flag| built || has_flag(&full_words, flag))
}

/// Whether `filtered` carries an option that configures the run differently
/// from `full` (`go test -tags integration`, `-race`, `pytest -c other.ini`,
/// `vitest --config e2e.ts`), so it proves what the full run does not. The
/// flags in [`BUILD_CONFIG_FLAGS`] are compared by `build_config` instead.
/// An option the full run lacks is the adapter's own selection flag (`-run`,
/// `-k`, `--lib`) when adding it makes the full run a filtered one; otherwise
/// it configures. An option both runs carry differs when the word after it in
/// `filtered` (`-tags b`) is a value the full run does not carry; a word that
/// makes the full run filtered once added after the option is an operand
/// (`./pkg`), not a value.
fn configures_differently(full: &Run, filtered: &Run) -> bool {
    let full_words = run_words(&full.argv);
    let words = run_words(&filtered.argv);
    words.iter().enumerate().any(|(i, word)| {
        let name = word.split_once('=').map_or(word.as_str(), |(name, _)| name);
        if !word.starts_with('-') || BUILD_CONFIG_FLAGS.contains(&name) {
            return false;
        }
        if !full_words.contains(word) {
            return full
                .adapter
                .is_full_run(&with_options(&full.argv, &[word.as_str()]));
        }
        let Some(next) = words.get(i + 1).filter(|next| !next.starts_with('-')) else {
            return false;
        };
        let carried = full_words
            .windows(2)
            .any(|pair| pair[0] == *word && pair[1] == *next);
        let probe = with_options(&full.argv, &[word.as_str(), next.as_str()]);
        !carried && full.adapter.is_full_run(&probe)
    })
}

/// Whether the full run `full` executes every test the filtered run `filtered`
/// selects, so the filtered criterion adds nothing. A full run that uses
/// `--skip` omits tests the filtered run may select, and a run built or
/// configured differently (features, profile, target, tags, config file, race
/// detector) compiles or runs different code. A missed warning costs less
/// than advice to drop a criterion that adds coverage, so a doubtful
/// difference counts.
fn subsumes(full: &Run, filtered: &Run) -> bool {
    if full.adapter.name() != filtered.adapter.name() || filtered.full {
        return false;
    }
    if full.argv.iter().any(|word| word == "--skip") {
        return false;
    }
    let stripped = strip_prefixes(&filtered.argv);
    if NOT_COVERED_FLAGS.iter().any(|f| has_flag(&stripped, f)) {
        return false;
    }
    environment(&full.argv) == environment(&filtered.argv)
        && build_config(&full.argv) == build_config(&filtered.argv)
        && unbuilt_targets_covered(full, filtered)
        && !configures_differently(full, filtered)
}

/// Warn when a stage's acceptance runs the whole test suite outside the
/// integration-verify stage; in integration-verify, warn on a filtered run a
/// full-suite criterion of the same runner already covers.
///
/// Called from `warn_ungrantable_acceptance`'s last statement (in
/// `validation.rs`), not directly from `validate_structural_preflight` -
/// that function is pinned at its recorded line count in
/// `maintainability-baseline.txt`, and routing through an existing call
/// keeps it from growing.
pub(super) fn warn_full_suite_outside_integration_verify(
    stage: &StageDefinition,
    resolved_type: StageType,
    warnings: &mut Vec<String>,
) {
    if resolved_type == StageType::IntegrationVerify {
        warn_subsumed_filtered_runs(stage, warnings);
        return;
    }
    for (idx, criterion) in stage.acceptance.iter().enumerate() {
        let command = criterion.command();
        let Some(run) = classify(command).filter(|run| run.full) else {
            continue;
        };
        let head: String = command.chars().take(80).collect();
        warnings.push(format!(
            "Stage '{}': Acceptance criterion #{} runs the whole test suite ({}) ('{}'); prove \
             this stage's own code with 'cargo test --lib <module>::' or a filtered run, and \
             leave the unfiltered suite to the integration-verify stage.",
            stage.id,
            idx + 1,
            run.adapter.name(),
            head
        ));
    }
}

/// Warn on each filtered run in an integration-verify stage that an earlier
/// or later full run of the same runner (same environment) already executes.
/// All criteria share the stage's `working_dir`.
fn warn_subsumed_filtered_runs(stage: &StageDefinition, warnings: &mut Vec<String>) {
    let runs: Vec<Option<Run>> = stage
        .acceptance
        .iter()
        .map(|criterion| classify(criterion.command()))
        .collect();
    for (idx, filtered) in runs.iter().enumerate() {
        let Some(filtered) = filtered else { continue };
        let covering = runs.iter().position(|run| {
            run.as_ref()
                .is_some_and(|full| full.full && subsumes(full, filtered))
        });
        let Some(full_idx) = covering else { continue };
        warnings.push(format!(
            "Stage '{}': Acceptance criterion #{} is a filtered {} run the full-suite \
             criterion #{} already covers; drop it, or move the proof into stdout_contains \
             on #{}",
            stage.id,
            idx + 1,
            filtered.adapter.name(),
            full_idx + 1,
            full_idx + 1
        ));
    }
}
