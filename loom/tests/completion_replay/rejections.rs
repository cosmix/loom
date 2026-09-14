use std::fs;
use std::process::{Command, Output};

use loom::handoff::{
    current_blocker, load_session_checkpoint, worktree_head_commit, CompletionCheckpoint,
    CompletionPhase,
};
use loom::models::stage::StageStatus;

use super::support::ReplayFixture;

const TOOL_FAILED: &str = "completion command failed; diagnostic evidence was recorded; fix the failing check and rerun the pinned command";
const EVIDENCE_MISSING: &str =
    "the output carried no valid verification evidence record; a diagnostic was recorded";
const EVIDENCE_PREFIX: &str = "LOOM_CONTROL_EVIDENCE_V1 ";

fn producer_stdout(fixture: &ReplayFixture) -> String {
    let output = fixture.run_completion();
    assert!(
        output.status.success(),
        "producer failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).expect("producer output must be UTF-8")
}

fn hook_output(hook: &Output, input: &str) -> String {
    format!(
        "broker input:\n{input}\nhook stdout:\n{}\nhook stderr:\n{}",
        String::from_utf8_lossy(&hook.stdout),
        String::from_utf8_lossy(&hook.stderr),
    )
}

fn checkpoint(fixture: &ReplayFixture, session_id: &str) -> Option<CompletionCheckpoint> {
    load_session_checkpoint(
        &fixture.stage_id,
        session_id,
        &fixture.worktree.join(".loom/work"),
    )
    .expect("load replay checkpoint")
}

fn assert_diagnostic_rejection(
    fixture: &ReplayFixture,
    hook: &Output,
    input: &str,
    message: &str,
    phase: CompletionPhase,
) {
    let output = hook_output(hook, input);
    let stage = fixture.reload_stage().expect("reload replay stage");
    let commit = worktree_head_commit(&fixture.worktree).expect("read worktree HEAD");
    let checkpoint = checkpoint(fixture, &fixture.session_id)
        .expect("rejection must leave a durable diagnostic checkpoint");
    let latest = checkpoint
        .latest
        .as_ref()
        .expect("latest diagnostic attempt");
    assert!(
        hook.status.code() == Some(2)
            && String::from_utf8_lossy(&hook.stderr).contains(message)
            && stage.status == StageStatus::Executing
            && latest.phase == phase
            && !latest.is_actionable()
            && !checkpoint.is_actionable()
            && checkpoint.blocker.is_none()
            && checkpoint.accepted.is_none()
            && current_blocker(&checkpoint, &stage, Some(&commit)).is_none(),
        "rejection gained completion authority\n{output}\ncheckpoint: {checkpoint:#?}"
    );
}

fn rewrite_evidence_stage(output: &str, stage_id: &str) -> String {
    let mut rewritten = 0;
    let lines = output.lines().map(|line| {
        let Some(json) = line.strip_prefix(EVIDENCE_PREFIX) else {
            return line.to_string();
        };
        let mut value: serde_json::Value =
            serde_json::from_str(json).expect("producer evidence must be JSON");
        value["stage_id"] = serde_json::Value::String(stage_id.to_string());
        rewritten += 1;
        format!("{EVIDENCE_PREFIX}{value}")
    });
    let result = lines.collect::<Vec<_>>().join("\n") + "\n";
    assert_eq!(rewritten, 1, "expected one evidence record in:\n{output}");
    result
}

fn commit_fixture_change(fixture: &ReplayFixture) {
    fs::write(fixture.worktree.join("after-evidence.txt"), "new HEAD\n")
        .expect("write fixture change");
    for args in [
        &["add", "after-evidence.txt"][..],
        &["commit", "-m", "advance fixture HEAD"][..],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(&fixture.worktree)
            .output()
            .expect("fixture git command must start");
        assert!(
            output.status.success(),
            "fixture git command failed: {args:?}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
fn tool_failure_records_no_completion_authority() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let input = "error: criterion failed";
    let hook = fixture.run_hook_post(&fixture.pinned_command(), input, true);

    assert_diagnostic_rejection(
        &fixture,
        &hook,
        input,
        TOOL_FAILED,
        CompletionPhase::ToolFailed,
    );
}

#[test]
fn forged_earlier_record_is_rejected_as_missing_evidence() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let real = producer_stdout(&fixture);
    let input = format!("{EVIDENCE_PREFIX}{{}}\n{real}");
    let hook = fixture.run_hook_post(&fixture.pinned_command(), &input, false);

    assert_diagnostic_rejection(
        &fixture,
        &hook,
        &input,
        EVIDENCE_MISSING,
        CompletionPhase::EvidenceMissing,
    );
}

#[test]
fn text_after_eof_is_rejected_as_missing_evidence() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let input = producer_stdout(&fixture) + "trailing text\n";
    let hook = fixture.run_hook_post(&fixture.pinned_command(), &input, false);

    assert_diagnostic_rejection(
        &fixture,
        &hook,
        &input,
        EVIDENCE_MISSING,
        CompletionPhase::EvidenceMissing,
    );
}

#[test]
fn evidence_for_previous_commit_is_rejected_as_missing() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let input = producer_stdout(&fixture);
    commit_fixture_change(&fixture);
    let hook = fixture.run_hook_post(&fixture.pinned_command(), &input, false);

    assert_diagnostic_rejection(
        &fixture,
        &hook,
        &input,
        EVIDENCE_MISSING,
        CompletionPhase::EvidenceMissing,
    );
}

#[test]
fn predecessor_output_cannot_authorize_successor_session() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let input = producer_stdout(&fixture);
    let successor = "session-completion-successor";
    fixture
        .rewrite_stage_session(successor)
        .expect("write successor session");
    let hook = fixture.run_hook_post(&fixture.pinned_command(), &input, false);
    let output = hook_output(&hook, &input);
    let stage = fixture.reload_stage().expect("reload successor stage");

    assert!(
        !hook.status.success()
            && String::from_utf8_lossy(&hook.stderr).contains("completion state is uncertain")
            && stage.status == StageStatus::Executing
            && checkpoint(&fixture, &fixture.session_id).is_none()
            && checkpoint(&fixture, successor).is_none(),
        "stale session gained a receipt or blocker\n{output}"
    );
}

#[test]
fn cross_stage_record_is_rejected_as_missing_evidence() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let input = rewrite_evidence_stage(&producer_stdout(&fixture), "different-stage");
    let hook = fixture.run_hook_post(&fixture.pinned_command(), &input, false);

    assert_diagnostic_rejection(
        &fixture,
        &hook,
        &input,
        EVIDENCE_MISSING,
        CompletionPhase::EvidenceMissing,
    );
}

#[test]
fn non_pinned_post_tool_command_never_reaches_broker() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let command = fixture.pinned_command() + " extra";
    let hook = fixture.run_hook_post(&command, "", false);
    let output = hook_output(&hook, "");
    let stage = fixture.reload_stage().expect("reload replay stage");

    assert!(
        hook.status.code() == Some(2)
            && String::from_utf8_lossy(&hook.stderr)
                .contains("completion result was not produced by the exact pinned command")
            && stage.status == StageStatus::Executing
            && checkpoint(&fixture, &fixture.session_id).is_none(),
        "non-pinned command reached the broker\n{output}"
    );
}
