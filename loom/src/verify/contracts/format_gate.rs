//! The formatter gate of `loom stage contracts freeze`: a frozen file cannot
//! change, so a contract or harness file that fails the stage's own formatter
//! checks must be fixed before the freeze, not disputed after it.
//!
//! [`is_format_check`] picks the stage's acceptance commands that only check
//! formatting; [`format_problems`] runs those through the acceptance runner,
//! so setup, `${..}` expansion, timeouts and confinement match
//! `loom stage complete`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::models::stage::{AcceptanceCriterion, CommandConfinement, Stage, StageSandboxConfig};
use crate::testrun::recognize::{command_args, has_flag, invocations};
use crate::verify::criteria::{run_acceptance_with_config, CriteriaConfig};

/// Formatters that check without rewriting when one of the flags is present:
/// the words that start the command, then the flags (any one suffices).
const FLAG_CHECKS: [(&[&str], &[&str]); 6] = [
    (&["cargo", "fmt"], &["--check"]),
    (&["rustfmt"], &["--check"]),
    (&["prettier"], &["--check", "-c"]),
    (&["oxfmt"], &["--check"]),
    (&["ruff", "format"], &["--check"]),
    (&["black"], &["--check"]),
];

/// Biome subcommands that only report unless one of [`BIOME_WRITE_FLAGS`] is set.
const BIOME_SUBCOMMANDS: [&[&str]; 2] = [&["biome", "format"], &["biome", "check"]];
const BIOME_WRITE_FLAGS: [&str; 3] = ["--write", "--fix", "--apply"];

/// A `format:check` package script, as the runner prefixes leave each form:
/// `yarn [run] format:check` reads `["format:check"]`.
const SCRIPT_CHECKS: [&[&str]; 5] = [
    &["format:check"],
    &["npm", "run", "format:check"],
    &["bun", "run", "format:check"],
    &["pnpm", "run", "format:check"],
    &["pnpm", "format:check"],
];

/// Whether `command`, run from `cwd`, is a formatter check: a simple command
/// that checks formatting without rewriting files. A compound command is one
/// only when every simple command in it is: the freeze runs while the
/// contracts are red, so the other half of `cargo fmt --check && cargo test`
/// would fail every freeze.
pub fn is_format_check(command: &str, cwd: &Path) -> bool {
    let argvs = invocations(command, cwd);
    !argvs.is_empty() && argvs.iter().all(|argv| is_format_check_argv(argv))
}

fn is_format_check_argv(argv: &[String]) -> bool {
    let flagged = FLAG_CHECKS.iter().any(|(words, flags)| {
        command_args(argv, words).is_some_and(|args| flags.iter().any(|f| has_flag(&args, f)))
    });
    flagged || is_biome_check(argv) || is_script_check(argv)
}

fn is_biome_check(argv: &[String]) -> bool {
    BIOME_SUBCOMMANDS.iter().any(|words| {
        command_args(argv, words)
            .is_some_and(|args| !BIOME_WRITE_FLAGS.iter().any(|f| has_flag(&args, f)))
    })
}

fn is_script_check(argv: &[String]) -> bool {
    SCRIPT_CHECKS
        .iter()
        .any(|words| command_args(argv, words).is_some())
}

/// The stage's acceptance commands that are formatter checks, run as
/// acceptance runs them (from `working_dir`, inside `worktree_root`); one
/// problem per check that fails, naming its command.
pub fn format_problems(
    acceptance: &[AcceptanceCriterion],
    setup: &[String],
    worktree_root: &Path,
    working_dir: &Path,
    confinement: CommandConfinement,
) -> Result<Vec<String>> {
    let checks: Vec<AcceptanceCriterion> = acceptance
        .iter()
        .filter(|criterion| is_format_check(criterion.command(), working_dir))
        .cloned()
        .collect();
    if checks.is_empty() {
        return Ok(Vec::new());
    }
    let stage = Stage {
        acceptance: checks,
        setup: setup.to_vec(),
        sandbox: StageSandboxConfig {
            command_confinement: Some(confinement),
            ..StageSandboxConfig::default()
        },
        ..Stage::default()
    };
    let config = CriteriaConfig::default().with_tree_root(Some(worktree_root));
    let outcome = run_acceptance_with_config(&stage, Some(working_dir), &config)
        .context("Failed to run the stage's formatter checks")?;
    Ok(outcome
        .results()
        .iter()
        .filter(|result| !result.passed())
        .map(|result| {
            format!(
                "acceptance criterion `{}` fails on the contract files: run the repository's \
                 formatter over them",
                result.command
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
