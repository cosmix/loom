//! The block-ledger half of the contracts: one blocked call per guard, the line it appends to
//! `<work>/heartbeat/<stage>.blocks.jsonl`, and the `loom hook blocks` report over that file.

use crate::support::{run_with_input, Agent, Fixture};
use crate::test_support;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

pub const STAGE: &str = "s";
pub const SESSION: &str = "x";

/// One call that must block, and the ledger line it must leave. The hook is `<guard>.sh`; the
/// recorded role is `subagent` for a subagent payload and `main` otherwise.
pub struct BlockCase<'a> {
    pub guard: &'a str,
    pub rule: &'a str,
    pub agent: Agent<'a>,
    pub payload: Value,
    pub env: Vec<(&'a str, String)>,
}

/// Every line of the ledger at `path`, each parsed as JSON; none when the file is absent.
pub fn ledger_lines(path: &Path) -> Vec<Value> {
    let Ok(body) = fs::read_to_string(path) else {
        return Vec::new();
    };
    body.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("ledger line {line:?} is no JSON: {error}"))
        })
        .collect()
}

/// Run `case` from the worktree and assert it blocked, tagged its message, and appended exactly
/// one ledger line naming its guard, rule, role, stage and session.
#[track_caller]
pub fn assert_block_recorded(fx: &Fixture, case: BlockCase, ledger: &Path) {
    let tag = format!("[{}/{}]", case.guard, case.rule);
    let role = match case.agent {
        Agent::Subagent(_) => "subagent",
        Agent::Plain => "main",
    };
    let hook = format!("{}.sh", case.guard);
    let before = ledger_lines(ledger).len();
    let run = fx.run(&hook, case.agent, case.payload, &fx.wt, &case.env);
    run.assert_code(2, &tag);
    assert!(
        run.first_stderr_line().starts_with(&tag),
        "{tag}: first non-empty stderr line lacks the tag: {}",
        run.describe()
    );
    let lines = ledger_lines(ledger);
    assert_eq!(
        lines.len(),
        before + 1,
        "{tag}: expected exactly one new ledger line in {}: {lines:?}",
        ledger.display()
    );
    let line = &lines[before];
    for (field, want) in [
        ("guard", case.guard),
        ("rule", case.rule),
        ("role", role),
        ("stage", STAGE),
        ("session", SESSION),
    ] {
        assert_eq!(line[field], want, "{tag}: ledger field {field}: {line}");
    }
}

/// `loom hook blocks --by-rule --since-hours 1` with every `LOOM_*` variable scrubbed except
/// `LOOM_WORK_DIR`. Returns stdout after asserting exit 0.
pub fn blocks_report(fx: &Fixture, work: &Path) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    command.args(["hook", "blocks", "--by-rule", "--since-hours", "1"]);
    test_support::clear_loom_env(&mut command);
    command
        .current_dir(&fx.root)
        .env("HOME", &fx.home)
        .env("LOOM_WORK_DIR", work);
    let output = run_with_input(&mut command, b"");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "loom hook blocks exited {}\nstdout: {stdout}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

#[track_caller]
pub fn assert_report_line(report: &str, expected: &str) {
    assert!(
        report.lines().any(|line| line.trim() == expected),
        "loom hook blocks lacks the line {expected:?}:\n{report}"
    );
}
