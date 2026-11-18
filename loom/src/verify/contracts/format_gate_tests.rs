use super::*;
use crate::models::stage::{AcceptanceCriterion, TruthCheck};
use tempfile::TempDir;

fn check(command: &str) -> bool {
    is_format_check(command, Path::new("."))
}

#[test]
fn every_listed_formatter_check_is_recognised() {
    for command in [
        "cargo fmt --check",
        "cargo fmt --all -- --check",
        "cargo +nightly fmt --all -- --check",
        "rustfmt --check src/lib.rs",
        "prettier --check .",
        "bunx prettier -c src",
        "oxfmt --check",
        "ruff format --check .",
        "uv run ruff format --check",
        "black --check .",
        "biome format .",
        "biome check .",
        "env CI=1 npx biome check src",
        "npm run format:check",
        "bun run format:check",
        "pnpm format:check",
        "pnpm run format:check",
        "yarn format:check",
        "yarn run format:check",
    ] {
        assert!(check(command), "{command} is a formatter check");
    }
}

#[test]
fn near_misses_are_not_formatter_checks() {
    for command in [
        "cargo fmt --all",
        "biome format --write .",
        "biome check --fix .",
        "prettier --write .",
        "ruff format .",
        "cargo test",
        "gofmt -l .",
        "cargo fmt --check && cargo test",
        "cargo fmt --check && npm run lint",
        "",
    ] {
        assert!(!check(command), "{command:?} is not a formatter check");
    }
}

#[test]
fn a_script_mixing_a_formatter_with_other_commands_is_not_a_check() {
    let temp = TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"test":"prettier --check . && vitest run"}}"#,
    )
    .unwrap();
    assert!(!is_format_check("npm test", temp.path()));
    std::fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"test":"prettier --check ."}}"#,
    )
    .unwrap();
    assert!(is_format_check("npm test", temp.path()));
}

fn crate_with(source: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let manifest =
        "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n";
    std::fs::write(temp.path().join("Cargo.toml"), manifest).unwrap();
    std::fs::create_dir(temp.path().join("src")).unwrap();
    std::fs::write(temp.path().join("src/lib.rs"), source).unwrap();
    temp
}

// Toolchain lookup reads process-wide HOME/PATH; callers share the environment-test lock.
fn problems_for(temp: &TempDir) -> Vec<String> {
    let _toolchain = crate::verify::test_support::ToolchainReadRoot::from_runner();
    let acceptance = [
        AcceptanceCriterion::Simple("cargo fmt --check".to_string()),
        AcceptanceCriterion::Simple("false".to_string()),
    ];
    let dir = temp.path().canonicalize().unwrap();
    format_problems(&acceptance, &[], &dir, &dir, CommandConfinement::Confined).unwrap()
}

#[test]
#[serial_test::serial]
fn an_unformatted_file_gives_one_problem_naming_the_check() {
    let problems = problems_for(&crate_with("pub fn  add(a:u32,b:u32)->u32{a+b}\n"));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("`cargo fmt --check`"), "{problems:?}");
}

#[test]
#[serial_test::serial]
fn a_formatted_file_gives_no_problem() {
    let formatted = "pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n";
    let problems = problems_for(&crate_with(formatted));
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn no_formatter_check_runs_nothing() {
    let acceptance = [AcceptanceCriterion::Simple("false".to_string())];
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let problems =
        format_problems(&acceptance, &[], dir, dir, CommandConfinement::Confined).unwrap();
    assert!(problems.is_empty());
}

fn pure(command: &str) -> bool {
    is_pure_check(command, Path::new("."))
}

#[test]
fn lints_and_searches_are_pure_checks() {
    for command in [
        "cargo clippy --all-targets -- -D warnings",
        "cargo +nightly clippy",
        "eslint src",
        "npx eslint .",
        "biome lint .",
        "ruff check .",
        "uv run ruff check src",
        "tsc --noEmit",
        "go vet ./...",
        "golangci-lint run",
        "rg -n FIXME tests/",
        "! rg -n FIXME tests/",
        "! grep -rn FIXME tests/",
        "grep -q todo notes.txt",
        "cargo fmt --check",
        "cargo clippy && rg -n FIXME src",
    ] {
        assert!(pure(command), "{command:?} is a pure check");
    }
}

#[test]
fn autofix_forms_and_other_commands_are_not_pure_checks() {
    for command in [
        "cargo clippy --fix",
        "cargo clippy --fix --allow-dirty",
        "eslint --fix .",
        "biome check --write",
        "biome lint --write .",
        "ruff check --fix",
        "ruff check --unsafe-fixes",
        "golangci-lint run --fix",
        "tsc",
        "tsc -p tsconfig.json",
        "cargo build",
        "cargo test",
        "cargo clippy && cargo test",
        "rg -n FIXME src | head",
        "",
    ] {
        assert!(!pure(command), "{command:?} is not a pure check");
    }
}

#[test]
fn compiler_backed_checks_wait_for_compiling_contracts() {
    let cwd = Path::new(".");
    for command in [
        "cargo clippy",
        "go vet ./...",
        "golangci-lint run",
        "rg -n x . && cargo clippy",
    ] {
        assert!(!runs_at_freeze(command, cwd, false), "{command:?} waits");
        assert!(
            runs_at_freeze(command, cwd, true),
            "{command:?} runs once they compile"
        );
    }
    for compiles in [false, true] {
        assert!(
            !runs_at_freeze("tsc --noEmit", cwd, compiles),
            "tsc never runs at freeze"
        );
    }
    for command in [
        "eslint src",
        "biome lint .",
        "ruff check .",
        "rg -n x .",
        "cargo fmt --check",
    ] {
        assert!(
            runs_at_freeze(command, cwd, false),
            "{command:?} always runs"
        );
    }
    assert!(!runs_at_freeze("cargo build", cwd, true));
}

fn written() -> Vec<String> {
    ["tests/foo.rs", "/w/pkg/tests/foo.rs", "src/lib.rs"]
        .map(String::from)
        .to_vec()
}

#[test]
fn attribution_needs_the_whole_path() {
    let blamed = |output: &str| attribute_failure(output, &written());
    assert_eq!(blamed("  --> tests/foo.rs:3:5"), ["tests/foo.rs"]);
    assert_eq!(blamed("./tests/foo.rs:3:5: unused"), ["tests/foo.rs"]);
    assert_eq!(
        blamed("tests/foo.rs"),
        ["tests/foo.rs"],
        "at the very start and end"
    );
    assert_eq!(
        blamed("--> /w/pkg/tests/foo.rs:1:1"),
        ["/w/pkg/tests/foo.rs"]
    );
    assert!(blamed("--> my_tests/foo.rs:3:5").is_empty(), "another file");
    assert!(blamed("--> ../tests/foo.rs:3:5").is_empty(), "a file above");
    assert!(blamed("--> tests/foo.rsx:1").is_empty(), "a longer name");
    assert!(blamed("--> tests/foo.rs_old:1").is_empty(), "a longer name");
    assert!(blamed("error in src/other.rs").is_empty());
    assert_eq!(
        blamed("src/lib.rs:1\ntests/foo.rs:2\nsrc/lib.rs:9"),
        ["src/lib.rs", "tests/foo.rs"],
        "sorted and deduplicated"
    );
    assert!(attribute_failure("anything", &[String::new()]).is_empty());
}

#[test]
fn only_pure_checks_the_gate_owns_are_selected() {
    let stage = Stage {
        acceptance: [
            "cargo fmt --check",
            "cargo clippy",
            "eslint src",
            "cargo test",
        ]
        .map(|command| AcceptanceCriterion::Simple(command.to_string()))
        .to_vec(),
        after_stage: vec![TruthCheck {
            command: "cargo fmt --check".to_string(),
            stdout_contains: Vec::new(),
            stdout_not_contains: Vec::new(),
            stderr_empty: None,
            exit_code: None,
            timeout_secs: None,
            description: None,
        }],
        ..Stage::default()
    };
    let chosen = selected_checks(&stage, Path::new("."), false);
    let names: Vec<(&str, &str)> = chosen
        .iter()
        .map(|(label, c)| (*label, c.command()))
        .collect();
    assert_eq!(
        names,
        [
            ("acceptance criterion", "eslint src"),
            ("after_stage criterion", "cargo fmt --check"),
        ],
        "acceptance formatters belong to format_problems; clippy waits for compiling contracts"
    );
    let AcceptanceCriterion::Extended(check) = &chosen[1].1 else {
        panic!("an after_stage check converts to an extended criterion");
    };
    assert_eq!(check.timeout_secs, Some(DEFAULT_CHECK_TIMEOUT.as_secs()));
}

fn search_problems(acceptance: &str, files: &[(&str, &str)], written: &[&str]) -> Vec<String> {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    for (name, content) in files {
        std::fs::write(dir.join(name), content).unwrap();
    }
    let stage = Stage {
        acceptance: vec![AcceptanceCriterion::Simple(acceptance.to_string())],
        ..Stage::default()
    };
    let written: Vec<String> = written.iter().map(|path| path.to_string()).collect();
    static_problems(
        &stage,
        &dir,
        &dir,
        CommandConfinement::Inherit,
        &written,
        true,
    )
    .unwrap()
}

#[test]
fn a_search_hit_in_a_written_file_is_one_problem() {
    let command = "! grep -Hn FIXME written.txt other.txt";
    let files = [("written.txt", "FIXME one\n"), ("other.txt", "FIXME two\n")];
    let problems = search_problems(command, &files, &["written.txt"]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("acceptance criterion `! grep"),
        "{problems:?}"
    );
    assert!(problems[0].contains("(written.txt)"), "{problems:?}");
    assert!(
        problems[0].contains("\nrun-output: "),
        "the output is quoted: {problems:?}"
    );
}

#[test]
fn a_search_hit_in_an_unwritten_file_gives_no_problem() {
    let command = "! grep -Hn FIXME written.txt other.txt";
    let files = [("written.txt", "clean\n"), ("other.txt", "FIXME two\n")];
    let problems = search_problems(command, &files, &["written.txt"]);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_passing_search_gives_no_problem() {
    let files = [("written.txt", "clean\n")];
    let problems = search_problems("! grep -Hn FIXME written.txt", &files, &["written.txt"]);
    assert!(problems.is_empty(), "{problems:?}");
}
