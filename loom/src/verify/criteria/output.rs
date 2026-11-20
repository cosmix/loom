//! Diagnostic rendering for a single acceptance criterion's result.

use crate::testrun::RunningTests;
use crate::verify::criteria::CriterionResult;

/// Number of trailing lines of stdout/stderr shown for a failed or timed-out
/// criterion.
const TAIL_LINES: usize = 20;

/// Most running tests a timed-out result names, and most characters of each.
const RUNNING_NAMES: usize = 20;
const RUNNING_NAME_CHARS: usize = 200;

/// Render one criterion's pass/fail/timeout line. `index` is the same index
/// `loom stage dispute-criteria --criterion-index` takes; `setup` is the
/// stage's setup commands, which ran first in the same shell as `result`'s
/// command, so a failing setup command fails every criterion and its stderr
/// is already folded into `result.stderr`.
///
/// A passing result renders exactly as before: one line, no captured output.
/// A FAILED or TIMEOUT result additionally shows the exit code, the setup
/// commands (when any ran), and the tails of stderr and stdout — otherwise
/// the cause of a failure stayed invisible until someone re-ran the command
/// by hand.
pub(crate) fn format_criterion_result(
    index: usize,
    result: &CriterionResult,
    setup: &[String],
) -> String {
    if result.success && result.cached {
        return format!("  ✓ passed (cached): {}", result.command);
    }
    if result.success {
        return format!("  ✓ passed: {}", result.command);
    }

    let label = if result.timed_out {
        format!("  ✗ TIMEOUT [criterion {index}]: {}", result.command)
    } else {
        format!("  ✗ FAILED [criterion {index}]: {}", result.command)
    };

    let mut lines = vec![label];

    lines.push(match result.exit_code {
        Some(code) => format!("    exit code: {code}"),
        None => "    exit code: none (the process was killed or timed out)".to_string(),
    });

    if !setup.is_empty() {
        lines.push(format!(
            "    setup (ran first, joined with &&): {}",
            setup.join(" && ")
        ));
    }

    if let Some(running) = result
        .running_at_timeout
        .as_ref()
        .filter(|_| result.timed_out)
    {
        lines.push(running_block(running));
    }

    if let Some(tail) = tail_block("stderr", &result.stderr) {
        lines.push(tail);
    }
    if let Some(tail) = tail_block("stdout", &result.stdout) {
        lines.push(tail);
    }

    lines.join("\n")
}

/// The tests a timed-out test run left running, at most [`RUNNING_NAMES`] of
/// them, each cut to [`RUNNING_NAME_CHARS`] characters, or a line saying
/// that loom cannot read them from this runner's output.
pub(crate) fn running_block(running: &RunningTests) -> String {
    let names = match running {
        RunningTests::Named(names) if !names.is_empty() => names,
        RunningTests::Named(_) => {
            return "    still running at the timeout: no test named in the output".into()
        }
        RunningTests::Unknown => {
            return "    still running at the timeout: unknown, loom cannot read this \
                    runner's output for running tests; see its tail"
                .into()
        }
    };
    let mut block = vec![format!(
        "    still running at the timeout ({}):",
        names.len()
    )];
    for name in names.iter().take(RUNNING_NAMES) {
        let cut: String = name.chars().take(RUNNING_NAME_CHARS).collect();
        block.push(format!("      {cut}"));
    }
    if names.len() > RUNNING_NAMES {
        block.push(format!("      and {} more", names.len() - RUNNING_NAMES));
    }
    block.join("\n")
}

/// Render the last [`TAIL_LINES`] lines of `text` under a header naming the
/// stream, noting how many earlier lines were omitted when truncated.
/// Returns `None` when `text` is empty after trimming.
fn tail_block(stream: &str, text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let all_lines: Vec<&str> = trimmed.lines().collect();
    let omitted = all_lines.len().saturating_sub(TAIL_LINES);
    let tail = &all_lines[omitted..];

    let header = if omitted > 0 {
        format!("    {stream} (last {TAIL_LINES} lines, {omitted} earlier omitted):")
    } else {
        format!("    {stream}:")
    };

    let mut block = vec![header];
    block.extend(tail.iter().map(|line| format!("      {line}")));
    Some(block.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn result(
        success: bool,
        exit_code: Option<i32>,
        stderr: &str,
        timed_out: bool,
    ) -> CriterionResult {
        CriterionResult::new(
            "cargo test".to_string(),
            success,
            String::new(),
            stderr.to_string(),
            exit_code,
            Duration::from_millis(100),
            timed_out,
        )
    }

    #[test]
    fn failed_result_shows_exit_code_setup_and_stderr() {
        let r = result(
            false,
            Some(1),
            "mkdir: cannot create directory '/tmp/loom-pre-commit-plan': Read-only file system",
            false,
        );
        let setup = vec!["mkdir -p /tmp/loom-pre-commit-plan".to_string()];

        let out = format_criterion_result(3, &r, &setup);

        assert!(out.contains("[criterion 3]"), "{out}");
        assert!(out.contains("exit code: 1"), "{out}");
        assert!(
            out.contains(
                "mkdir: cannot create directory '/tmp/loom-pre-commit-plan': Read-only file system"
            ),
            "{out}"
        );
        assert!(out.contains("mkdir -p /tmp/loom-pre-commit-plan"), "{out}");
    }

    #[test]
    fn passing_result_with_stderr_renders_one_line_only() {
        let r = result(true, Some(0), "warning: something noisy", false);

        let out = format_criterion_result(0, &r, &[]);

        assert_eq!(out, "  ✓ passed: cargo test");
        assert!(!out.contains("something noisy"));
    }

    #[test]
    fn long_stderr_is_truncated_to_the_tail() {
        let lines: Vec<String> = (1..=50).map(|n| format!("line {n}")).collect();
        let r = result(false, Some(1), &lines.join("\n"), false);

        let out = format_criterion_result(0, &r, &[]);

        assert!(out.contains("30 earlier omitted"), "{out}");
        assert!(out.contains("line 50"), "{out}");
        assert!(out.contains("line 31"), "{out}");
        assert!(!out.contains("line 29"), "{out}");
    }

    #[test]
    fn timeout_result_carries_the_timeout_label_and_tails() {
        let r = result(false, None, "still running when killed", true);

        let out = format_criterion_result(2, &r, &[]);

        assert!(out.contains("TIMEOUT [criterion 2]"), "{out}");
        assert!(out.contains("exit code: none"), "{out}");
        assert!(out.contains("still running when killed"), "{out}");
        assert!(
            !out.contains("still running at the timeout"),
            "no test run: {out}"
        );
    }

    #[test]
    fn a_test_run_whose_runner_cannot_tell_says_so() {
        let mut r = result(false, None, "", true);
        r.running_at_timeout = Some(RunningTests::Unknown);

        let out = format_criterion_result(1, &r, &[]);

        assert!(
            out.contains("unknown, loom cannot read this runner's output for running tests"),
            "{out}"
        );
    }

    #[test]
    fn a_timeout_names_its_running_tests_within_bounds() {
        let mut r = result(false, None, "", true);
        let mut names: Vec<String> = (0..25).map(|n| format!("t{n}")).collect();
        names[0] = "x".repeat(500);
        r.running_at_timeout = Some(RunningTests::Named(names));

        let out = format_criterion_result(0, &r, &[]);

        assert!(out.contains("still running at the timeout (25):"), "{out}");
        assert!(out.contains(&"x".repeat(RUNNING_NAME_CHARS)), "{out}");
        assert!(!out.contains(&"x".repeat(RUNNING_NAME_CHARS + 1)), "{out}");
        assert!(out.contains("      t19\n      and 5 more"), "{out}");
        assert!(!out.contains("t20"), "{out}");
        r.running_at_timeout = Some(RunningTests::Named(Vec::new()));
        let none = format_criterion_result(0, &r, &[]);
        assert!(none.contains("no test named in the output"), "{none}");
    }
}
