//! Detects an unfiltered `cargo test` or `cargo nextest run` and warns when
//! it appears outside the integration-verify stage. Split out of
//! `validation.rs` (same reason as `structural_checks.rs`): a standard
//! stage's acceptance should prove the code that stage wrote, not re-run the
//! whole suite - the unfiltered run belongs to integration-verify alone, per
//! `skills/loom-plan-writer/SKILL.md` ("The full suite runs once, in
//! integration-verify").

use super::types::{StageDefinition, StageType};

/// Options that consume the following token as their value, so it is never
/// itself scanned as a name filter or scoping flag. Shared between `cargo
/// test` and `cargo nextest run`: the last three (`-E`, `--filter-expr`,
/// `--filterset`) are nextest's own filter-expression flags.
const VALUE_TAKING_OPTIONS: &[&str] = &[
    "--manifest-path",
    "-p",
    "--package",
    "--features",
    "--target-dir",
    "--profile",
    "--jobs",
    "-j",
    "--test",
    "--bin",
    "--example",
    "--bench",
    "-E",
    "--filter-expr",
    "--filterset",
];

/// Options that, on their own, scope the run to less than the whole suite -
/// for both `cargo test` and `cargo nextest run`.
const SCOPING_OPTIONS: &[&str] = &[
    "--lib",
    "--test",
    "--bin",
    "--example",
    "--bench",
    "--doc",
    "-p",
    "--package",
    "-E",
    "--filter-expr",
    "--filterset",
];

/// libtest options after a `cargo test --` that scope the run on their own:
/// `--ignored` runs only ignored tests, `--exact` narrows a name filter to an
/// exact match. `--skip <name>` is deliberately absent - excluding a handful
/// of tests still runs the rest of the suite, so a `-- --skip a --skip b`
/// run alone stays flagged.
const POST_DASH_SCOPING_FLAGS: &[&str] = &["--ignored", "--exact"];

/// Truncate `command` at the earliest of `|`, `&&`, `;`, or `2>&1` - the
/// piece before any of these is what the test runner itself actually sees.
fn command_head(command: &str) -> &str {
    let mut end = command.len();
    for delimiter in ["|", "&&", ";", "2>&1"] {
        if let Some(idx) = command.find(delimiter) {
            end = end.min(idx);
        }
    }
    &command[..end]
}

/// The token index just past a `cargo test` or `cargo nextest run`
/// invocation found anywhere in `tokens`, or `None` when neither appears.
fn runner_start(tokens: &[&str]) -> Option<usize> {
    for (idx, token) in tokens.iter().enumerate() {
        if *token != "cargo" {
            continue;
        }
        if tokens.get(idx + 1) == Some(&"test") {
            return Some(idx + 2);
        }
        if tokens.get(idx + 1) == Some(&"nextest") && tokens.get(idx + 2) == Some(&"run") {
            return Some(idx + 3);
        }
    }
    None
}

/// Scans libtest-style options after a `--` separator: `--ignored`,
/// `--exact`, or a bare name filter scope the run; `--skip <name>` does not
/// (see [`POST_DASH_SCOPING_FLAGS`]).
fn post_dash_is_scoped(tokens: &[&str]) -> bool {
    let mut idx = 0;
    while idx < tokens.len() {
        let token = tokens[idx];
        if token == "--skip" {
            idx += 2;
            continue;
        }
        if POST_DASH_SCOPING_FLAGS.contains(&token) || !token.starts_with('-') {
            return true;
        }
        idx += 1;
    }
    false
}

/// True when `command` is an unfiltered `cargo test` or `cargo nextest run`
/// - the whole suite, not a package/target/name-filtered slice of it.
pub(super) fn is_full_suite_run(command: &str) -> bool {
    let head = command_head(command);
    let tokens: Vec<&str> = head.split_whitespace().collect();
    let Some(start) = runner_start(&tokens) else {
        return false;
    };
    if tokens[start..].contains(&"--no-run") {
        return false; // compiles (or lists) only; never executes a test
    }

    let mut scoped = false;
    let mut idx = start;
    while idx < tokens.len() {
        let token = tokens[idx];
        if token == "--" {
            scoped |= post_dash_is_scoped(&tokens[idx + 1..]);
            break;
        }
        if SCOPING_OPTIONS.contains(&token) {
            scoped = true;
        } else if !token.starts_with('-') {
            // A bare token before `--` is a test-name filter.
            scoped = true;
        }
        idx += if VALUE_TAKING_OPTIONS.contains(&token) {
            2 // skip the option's value
        } else {
            1
        };
    }

    !scoped
}

/// Warn when a stage's acceptance runs the whole test suite outside the
/// integration-verify stage.
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
        return;
    }
    for (idx, criterion) in stage.acceptance.iter().enumerate() {
        let command = criterion.command();
        if !is_full_suite_run(command) {
            continue;
        }
        let head: String = command.chars().take(80).collect();
        warnings.push(format!(
            "Stage '{}': Acceptance criterion #{} runs the whole test suite ('{}'); prove \
             this stage's own code with 'cargo test --lib <module>::' or a filtered run, and \
             leave the unfiltered suite to the integration-verify stage.",
            stage.id,
            idx + 1,
            head
        ));
    }
}
