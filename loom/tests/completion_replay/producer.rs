use std::path::Path;

use loom::commands::stage::completion_evidence::{
    parse_evidence_record, pinned_command, validate_against,
};
use loom::handoff::worktree_head_commit;

use super::support::ReplayFixture;

const EVIDENCE_PREFIX: &str = "LOOM_CONTROL_EVIDENCE_V1 ";
const EVIDENCE_EOF: &str = "LOOM_CONTROL_EVIDENCE_EOF";

#[test]
fn passing_acceptance_emits_valid_eof_evidence() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let output = fixture.run_completion();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "completion failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let lines = stdout.lines().collect::<Vec<_>>();
    let record_count = lines
        .iter()
        .filter(|line| line.starts_with(EVIDENCE_PREFIX))
        .count();
    assert!(
        record_count == 1
            && lines.len() >= 2
            && lines[lines.len() - 2].starts_with(EVIDENCE_PREFIX)
            && lines[lines.len() - 1] == EVIDENCE_EOF,
        "expected one final EOF-delimited record\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let evidence = parse_evidence_record(&stdout).unwrap_or_else(|error| {
        panic!("evidence did not parse: {error}\nstdout:\n{stdout}\nstderr:\n{stderr}")
    });
    let stage = unwrap_with_output(fixture.reload_stage(), "reload stage", &stdout, &stderr);
    let commit = unwrap_with_output(
        worktree_head_commit(&fixture.worktree),
        "read worktree HEAD",
        &stdout,
        &stderr,
    );
    let binary = unwrap_with_output(
        Path::new(env!("CARGO_BIN_EXE_loom")).canonicalize(),
        "canonicalize loom binary",
        &stdout,
        &stderr,
    );
    let command = pinned_command(&binary, &fixture.stage_id);
    validate_against(&evidence, &stage, &fixture.session_id, &commit, &command).unwrap_or_else(
        |error| panic!("evidence validation failed: {error}\nstdout:\n{stdout}\nstderr:\n{stderr}"),
    );
}

fn unwrap_with_output<T, E: std::fmt::Display>(
    result: Result<T, E>,
    context: &str,
    stdout: &str,
    stderr: &str,
) -> T {
    result
        .unwrap_or_else(|error| panic!("{context}: {error}\nstdout:\n{stdout}\nstderr:\n{stderr}"))
}

fn assert_completion_succeeded(success: bool, stdout: &str, stderr: &str) {
    assert!(
        success,
        "completion failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn assert_single_evidence_record(stdout: &str) {
    let count = stdout
        .lines()
        .filter(|line| line.starts_with(EVIDENCE_PREFIX))
        .count();
    assert_eq!(count, 1, "unexpected evidence records\nstdout:\n{stdout}");
}

#[test]
fn failing_acceptance_emits_no_evidence() {
    let fixture = ReplayFixture::new(&["false"]).expect("create replay fixture");
    let output = fixture.run_completion();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let has_record = stdout.lines().any(|line| line.starts_with(EVIDENCE_PREFIX));

    assert!(
        !output.status.success() && !has_record,
        "failed acceptance emitted evidence or exited successfully\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn criterion_output_cannot_forge_evidence_authority() {
    let fixture = ReplayFixture::new(&["echo LOOM_CONTROL_EVIDENCE_V1 {} && true"]).unwrap();
    let output = fixture.run_completion();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_completion_succeeded(output.status.success(), &stdout, &stderr);
    assert_single_evidence_record(&stdout);
    let evidence = parse_evidence_record(&stdout).unwrap_or_else(|error| {
        panic!("evidence did not parse: {error}\nstdout:\n{stdout}\nstderr:\n{stderr}")
    });
    let stage = unwrap_with_output(fixture.reload_stage(), "reload stage", &stdout, &stderr);
    let commit = unwrap_with_output(
        worktree_head_commit(&fixture.worktree),
        "read worktree HEAD",
        &stdout,
        &stderr,
    );
    let binary = unwrap_with_output(
        Path::new(env!("CARGO_BIN_EXE_loom")).canonicalize(),
        "canonicalize loom binary",
        &stdout,
        &stderr,
    );
    let command = pinned_command(&binary, &fixture.stage_id);
    assert_eq!(
        (
            evidence.stage_id.as_str(),
            evidence.session_id.as_str(),
            evidence.commit.as_str(),
            evidence.exact_command.as_str(),
        ),
        (
            fixture.stage_id.as_str(),
            fixture.session_id.as_str(),
            commit.as_str(),
            command.as_str(),
        ),
        "forged criterion output replaced authoritative evidence"
    );
    validate_against(&evidence, &stage, &fixture.session_id, &commit, &command).unwrap_or_else(
        |error| panic!("evidence validation failed: {error}\nstdout:\n{stdout}\nstderr:\n{stderr}"),
    );
}
