//! The testrun core's named tests (DESIGN D5) and the shared helpers' tests.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::recognize::{
    combined_output, command_args, count_matches, has_flag, invocations, last_match, pattern,
    positionals, split_double_dash, strip_ansi, strip_prefixes, sum_matches, summary_from_counts,
};
use super::{
    classify, fixture_support, registry, RunOutcome, RunOutput, RunSummary, TestRunnerAdapter,
};

/// The adapter names of DESIGN D5.
const ADAPTER_NAMES: [&str; 23] = [
    "cargo-test",
    "cargo-nextest",
    "go-test",
    "pytest",
    "unittest",
    "vitest",
    "jest",
    "mocha",
    "bun-test",
    "node-test",
    "gradle",
    "maven",
    "sbt",
    "dotnet-test",
    "rspec",
    "minitest",
    "phpunit",
    "pest",
    "swift-test",
    "mix-test",
    "ctest",
    "dart-test",
    "flutter-test",
];

/// Scenarios every adapter records; compiled runners add `build-error`.
const REQUIRED_SCENARIOS: [&str; 4] = ["one-pass", "one-fail", "no-match", "suite"];

fn words(command: &str) -> Vec<String> {
    command.split_whitespace().map(str::to_string).collect()
}

fn recognized(command: &str, cwd: &Path) -> Option<&'static str> {
    registry::recognize(command, cwd).map(|adapter| adapter.name())
}

fn write_manifest(dir: &Path, json: &str) {
    let path = dir.join("package.json");
    fs::write(&path, json).expect("write package.json");
}

#[test]
fn registry_lists_every_adapter() {
    let registered: BTreeSet<&str> = registry::all().iter().map(|a| a.name()).collect();
    let expected: BTreeSet<&str> = ADAPTER_NAMES.into_iter().collect();
    assert_eq!(registered, expected);
    let count = registry::all().len();
    assert_eq!(count, ADAPTER_NAMES.len(), "an adapter is registered twice");
    for name in ADAPTER_NAMES {
        assert_eq!(registry::by_name(name).map(|a| a.name()), Some(name));
    }
}

#[test]
fn every_fixture_classifies_as_expected() {
    let problems: Vec<String> = registry::all()
        .iter()
        .flat_map(|adapter| fixture_problems(*adapter))
        .collect();
    let report = problems.join("\n");
    assert!(problems.is_empty(), "fixture mismatches:\n{report}");
}

fn fixture_problems(adapter: &dyn TestRunnerAdapter) -> Vec<String> {
    let name = adapter.name();
    let scenarios = fixture_support::scenarios(name);
    let mut problems: Vec<String> = REQUIRED_SCENARIOS
        .iter()
        .filter(|required| !scenarios.iter().any(|scenario| scenario == *required))
        .map(|missing| format!("{name}: no `{missing}` fixture"))
        .collect();
    for scenario in &scenarios {
        let (stdout, stderr, exit_code) = fixture_support::load(name, scenario);
        let output = RunOutput {
            stdout: &stdout,
            stderr: &stderr,
            exit_code,
        };
        if let Err(problem) = check_scenario(scenario, &adapter.parse(&output), exit_code) {
            problems.push(format!("{name}/{scenario}: {problem}"));
        }
    }
    problems
}

/// One scenario's parse against DESIGN D5. A variant (`one-pass.nocolor`,
/// `no-match.k`) carries its base scenario's expectation.
fn check_scenario(scenario: &str, summary: &RunSummary, exit: Option<i32>) -> Result<(), String> {
    let base = scenario.split('.').next().unwrap_or(scenario);
    let expected = match base {
        "one-pass" => RunOutcome::Passed,
        "one-fail" | "suite" => RunOutcome::Failed,
        "no-match" => RunOutcome::NotSelected,
        "build-error" => RunOutcome::BuildFailed,
        other => return Err(format!("unknown scenario `{other}`")),
    };
    let outcome = classify(summary, exit);
    if outcome != expected {
        let parsed = format!("{summary:?}");
        return Err(format!(
            "{outcome:?}, expected {expected:?}; parsed {parsed}"
        ));
    }
    if base == "suite" && (summary.executed, summary.failed) != (Some(3), Some(1)) {
        return Err(format!(
            "suite needs executed 3, failed 1; parsed {summary:?}"
        ));
    }
    Ok(())
}

#[test]
fn recognizes_prefixed_invocations() {
    let cwd = Path::new(".");
    for command in [
        "cd loom && cargo test --manifest-path loom/Cargo.toml x",
        "env RUST_LOG=1 cargo test",
        "cargo +nightly test",
    ] {
        assert_eq!(recognized(command, cwd), Some("cargo-test"), "{command}");
    }
    assert_eq!(recognized("echo cargo test", cwd), None);
}

#[test]
fn full_run_detection_ignores_filtered_runs() {
    let adapter = registry::by_name("cargo-test").expect("registered");
    for full in [
        "cargo test --all-targets",
        "cargo test",
        "cargo test --manifest-path loom/Cargo.toml -- --exact",
        "env CI=1 cargo +nightly test --workspace",
    ] {
        assert!(adapter.is_full_run(&words(full)), "{full}");
    }
    for filtered in [
        "cargo test --lib x::",
        "cargo test foo",
        "cargo test --manifest-path loom/Cargo.toml x",
        "cargo test -- foo",
        "cargo test --test integration",
        "cargo test -- --skip slow",
        "cargo build",
    ] {
        assert!(!adapter.is_full_run(&words(filtered)), "{filtered}");
    }
}

#[test]
fn package_script_indirection_is_recognized() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path();
    write_manifest(cwd, r#"{"scripts": {"test": "cargo test"}}"#);
    for command in [
        "npm test",
        "npm run test",
        "bun run test",
        "pnpm test",
        "yarn test",
        "CI=1 npm test",
    ] {
        assert_eq!(recognized(command, cwd), Some("cargo-test"), "{command}");
    }
    let empty = tempfile::tempdir().expect("temp dir");
    assert_eq!(recognized("npm test", empty.path()), None);
}

#[test]
fn invocations_follow_cd_and_pass_script_arguments() {
    let root = tempfile::tempdir().expect("temp dir");
    let web = root.path().join("web");
    fs::create_dir(&web).expect("create web");
    write_manifest(&web, r#"{"scripts": {"test": "tsc && cargo test"}}"#);
    let argvs = invocations("cd web && npm test -- --lib", root.path());
    assert_eq!(argvs, [words("tsc"), words("cargo test --lib")]);

    write_manifest(&web, r#"{"scripts": {"test": "npm test"}}"#);
    let argvs = invocations("npm test", &web);
    assert_eq!(argvs, [words("npm test")]);
}

#[test]
fn strip_prefixes_removes_runner_wrappers() {
    let cases = [
        ("env -u HOME CI=1 cargo +stable test", "cargo test"),
        ("RUST_LOG=debug cargo test", "cargo test"),
        ("uv run --with pytest python -m pytest -q", "pytest -q"),
        ("python3 -m unittest discover", "unittest discover"),
        ("bunx --bun vitest run", "vitest run"),
        ("npx -y jest", "jest"),
        ("pnpm exec vitest", "vitest"),
        ("yarn run mocha", "mocha"),
        ("bundle exec rspec spec", "rspec spec"),
        ("echo cargo test", "echo cargo test"),
    ];
    for (command, expected) in cases {
        let stripped = strip_prefixes(&words(command));
        assert_eq!(stripped.join(" "), expected, "{command}");
    }
    let ctest = command_args(&words("/usr/bin/ctest -R x"), &["ctest"]);
    assert_eq!(ctest, Some(words("-R x")));
    let bare = command_args(&words("cargo"), &["cargo", "test"]);
    assert_eq!(bare, None);
}

#[test]
fn flag_helpers_read_options_and_operands() {
    let args = words("--manifest-path m.toml --color=never x:: -- --exact y");
    assert!(has_flag(&args, "--color"));
    assert!(!has_flag(&args, "--colo"));
    let found = positionals(&args, &["--manifest-path"]);
    assert_eq!(found, ["x::", "--exact", "y"]);
    let (before, after) = split_double_dash(&args);
    assert_eq!(before.len(), 4);
    assert_eq!(after, words("--exact y"));
}

#[test]
fn output_helpers_sum_count_and_strip() {
    let passed = pattern(r"(\d+) passed");
    let text = "1 passed\n2 passed\n";
    assert_eq!(sum_matches(&passed, text), Some(3));
    assert_eq!(last_match(&passed, text), Some(2));
    assert_eq!(count_matches(&passed, text), 2);
    assert_eq!(sum_matches(&passed, "nothing"), None);
    assert_eq!(strip_ansi("\x1b[32mok\x1b[0m"), "ok");
    let output = RunOutput {
        stdout: "\x1b[1mout\x1b[0m",
        stderr: "err",
        exit_code: Some(0),
    };
    assert_eq!(combined_output(&output), "out\nerr");
    let summary = summary_from_counts(Some(2), Some(1), None);
    assert_eq!((summary.executed, summary.build_failed), (Some(3), false));
    assert_eq!(summary_from_counts(None, None, Some(4)).executed, None);
}
