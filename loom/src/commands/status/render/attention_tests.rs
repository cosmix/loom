use super::*;
use crate::commands::status::data::{
    ActivityStatus, CompletionBlockerState, CompletionBlockerSummary, StageType,
};
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::SessionExitReason;
use crate::models::stage::StageStatus;
use crate::testrun::recognize::strip_ansi;
use chrono::Utc;

fn make_stage_summary(id: &str, status: StageStatus) -> StageSummary {
    StageSummary {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        summary: None,
        status,
        stage_type: StageType::Standard,
        dependencies: vec![],
        context_tokens: None,
        elapsed_secs: None,
        execution_secs: None,
        base_branch: None,
        base_merged_from: vec![],
        failure_info: None,
        activity_status: ActivityStatus::default(),
        last_tool: None,
        last_activity: None,
        staleness_secs: None,
        context_ceiling_tokens: None,
        review_reason: None,
        review_notes: None,
        merged: false,
        merge_assumed: false,
        cleanup_warning: None,
        merge_block: None,
        stash_warning: None,
        held: false,
        retry_count: 0,
        max_retries: None,
        pid: None,
        session_alive: false,
        model: "opus".to_string(),
        session_type: None,
        incoherence: None,
        execution_models: vec![],
        dispute_count: 0,
        judge_heartbeat_secs: None,
        session_backend: None,
        outgoing_session_exit_reason: None,
        completion_blocker: None,
        merge_resolver_session: None,
        merge_resolver_attempts: None,
        close_reason: None,
        acceptance: vec![],
        terminal_history_session: None,
    }
}

fn completion_blocker(fingerprint: &str, next_action: &str) -> CompletionBlockerSummary {
    CompletionBlockerSummary {
        state: CompletionBlockerState::Blocked,
        fingerprint: fingerprint.to_string(),
        failure_code: "sandbox_denied".to_string(),
        summary: Some("sandbox denied execution".to_string()),
        commit: "0123456789ab".to_string(),
        repeat_count: 2,
        first_observed_at: Some("2026-09-14T10:00:00Z".to_string()),
        last_observed_at: Some("2026-09-14T10:01:00Z".to_string()),
        next_action: next_action.to_string(),
    }
}

fn rendered(stages: &[StageSummary]) -> String {
    let mut output = Vec::new();
    render_attention(&mut output, stages, false).unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn needs_human_review_renders_the_three_full_review_commands() {
    let stages = vec![make_stage_summary(
        "integration-verify",
        StageStatus::NeedsHumanReview,
    )];
    let output_str = rendered(&stages);

    for (command, description) in [
        (
            "loom stage human-review integration-verify --approve",
            "queue a fresh session with fresh fix attempts",
        ),
        (
            "loom stage human-review integration-verify --force-complete",
            "skip acceptance and mark completed",
        ),
        (
            "loom stage human-review integration-verify --reject \"<reason>\"",
            "block the stage",
        ),
    ] {
        assert!(output_str.contains(command), "output: {output_str}");
        assert!(output_str.contains(description), "output: {output_str}");
    }
    // Label substrings, not `Label:`: a colored label is followed by an escape
    // sequence when stdout is a terminal.
    assert!(
        output_str.matches("Run").count() == 1 && !output_str.contains("Note"),
        "a review entry has no single command or note line: {output_str}"
    );
}

#[test]
fn an_automatic_entry_renders_its_note_and_no_command() {
    let mut stage = make_stage_summary("docs", StageStatus::MergeConflict);
    stage.merge_resolver_session = Some("session-abc".to_string());
    stage.merge_resolver_attempts = Some(1);
    let output_str = rendered(&[stage]);

    assert!(
        output_str.contains("merge resolver session-abc is working (attempt 1 of 6)"),
        "output: {output_str}"
    );
    assert!(!output_str.contains("Run"), "output: {output_str}");
    assert!(
        !output_str.contains("loom stage merge"),
        "output: {output_str}"
    );
}

#[test]
fn blocked_renders_the_retry_hint_and_no_human_review_choices() {
    let stages = vec![make_stage_summary("stuck-stage", StageStatus::Blocked)];
    let mut output = Vec::new();
    render_attention(&mut output, &stages, false).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(
        output_str.contains("loom stage retry stuck-stage"),
        "output: {output_str}"
    );
    assert!(
        !output_str.contains("--approve"),
        "a Blocked stage must not show the human-review choices: {output_str}"
    );
    assert!(
        !output_str.contains("--force-complete"),
        "a Blocked stage must not show the human-review choices: {output_str}"
    );
}

#[test]
fn evidence_listing_is_gated_behind_verbose() {
    let mut stage = make_stage_summary("blocked-stage", StageStatus::Blocked);
    stage.failure_info = Some(FailureInfo {
        failure_type: FailureType::TestFailure,
        detected_at: Utc::now(),
        evidence: vec![
            "first evidence line".to_string(),
            "second evidence line".to_string(),
        ],
    });
    let stages = vec![stage];

    let mut quiet = Vec::new();
    render_attention(&mut quiet, &stages, false).unwrap();
    let quiet_str = String::from_utf8(quiet).unwrap();
    assert!(
        !quiet_str.contains("Evidence:"),
        "non-verbose output must not show the Evidence: listing: {quiet_str}"
    );
    assert!(
        !quiet_str.contains("first evidence line"),
        "non-verbose output must not show evidence lines: {quiet_str}"
    );

    let mut verbose = Vec::new();
    render_attention(&mut verbose, &stages, true).unwrap();
    let verbose_str = String::from_utf8(verbose).unwrap();
    assert!(verbose_str.contains("Evidence:"), "output: {verbose_str}");
    assert!(
        verbose_str.contains("first evidence line"),
        "output: {verbose_str}"
    );
    assert!(
        verbose_str.contains("second evidence line"),
        "output: {verbose_str}"
    );
}

#[test]
fn adjudication_reason_includes_the_judge_heartbeat_when_known() {
    let mut stage = make_stage_summary("s-adjudicate", StageStatus::NeedsAdjudication);
    stage.dispute_count = 2;
    stage.judge_heartbeat_secs = Some(42);
    let stages = vec![stage];
    let mut output = Vec::new();
    render_attention(&mut output, &stages, false).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(
        output_str.contains("2 disputes filed; judge heartbeat 42s ago"),
        "output: {output_str}"
    );
}

#[test]
fn cleanup_warning_renders_as_a_single_line_with_a_stage_file_hint() {
    // `sanitize_stage_summary` always flattens `cleanup_warning` to one line
    // before it reaches a real `AttentionEntry`, but this test bypasses that
    // and feeds an embedded newline directly to prove `render_cleanup_warning`
    // itself no longer special-cases multiple lines - the dead `.lines()`
    // loop this pins the removal of would otherwise indent each line again.
    // The assertion below is split in two because `warning.yellow()` inserts
    // an escape sequence between the label and the text when stdout is a
    // terminal (as in the pre-push hook), and a substring spanning that
    // boundary only matches under a pipe.
    let mut stage = make_stage_summary("cleanup-stage", StageStatus::Completed);
    stage.cleanup_warning = Some("failed: worktree busy\nretrying next cycle".to_string());
    let stages = vec![stage];
    let mut output = Vec::new();
    render_attention(&mut output, &stages, false).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(
        output_str.contains("Cleanup warning: "),
        "output: {output_str}"
    );
    assert!(
        output_str.contains("failed: worktree busy\nretrying next cycle"),
        "output: {output_str}"
    );
    assert!(
        output_str.contains("full text in the stage file"),
        "output: {output_str}"
    );
}

#[test]
fn adjudication_reason_omits_the_heartbeat_when_unknown() {
    let mut stage = make_stage_summary("s-adjudicate", StageStatus::NeedsAdjudication);
    stage.dispute_count = 2;
    stage.judge_heartbeat_secs = None;
    let stages = vec![stage];
    let mut output = Vec::new();
    render_attention(&mut output, &stages, false).unwrap();
    let output_str = String::from_utf8(output).unwrap();

    assert!(
        output_str.contains("2 disputes filed") && !output_str.contains("judge heartbeat"),
        "output: {output_str}"
    );
}

#[test]
fn completion_blockers_render_recovery_details_and_distinct_exit_reasons() {
    let mut stalled = make_stage_summary("stalled-writer", StageStatus::Executing);
    stalled.completion_blocker = Some(completion_blocker(
        "feedface1234",
        "confirm the sandbox grant, then retry",
    ));
    stalled.outgoing_session_exit_reason = Some(SessionExitReason::Stalled);
    let mut ceiling = make_stage_summary("ceiling-writer", StageStatus::NeedsHumanReview);
    ceiling.completion_blocker = Some(completion_blocker(
        "deadbeef5678",
        "start a continuation session",
    ));
    ceiling.outgoing_session_exit_reason = Some(SessionExitReason::ContextCeiling);

    let mut output = Vec::new();
    render_attention(&mut output, &[stalled, ceiling], false).unwrap();
    let output = String::from_utf8(output).unwrap();

    assert!(
        output.contains("Note")
            && output.contains("confirm the sandbox grant, then retry")
            && output.contains("fingerprint:")
            && output.contains("feedface1234")
            && output.contains("failure code:")
            && output.contains("sandbox_denied")
            && output.contains("last session: stalled")
            && output.contains("last session: context ceiling"),
        "output: {output}"
    );
}

/// The guidance as plain text: `colored` emits escapes whenever stdout is a
/// terminal, and the tests below compare whole lines and their widths.
fn guidance_output(stage: StageSummary, width: usize) -> String {
    let entry = attention_entries(&[stage]).remove(0);
    let mut output = Vec::new();
    write_guidance(&mut output, &entry, width).unwrap();
    strip_ansi(&String::from_utf8(output).unwrap()).into_owned()
}

const LONG_REASON: &str = "Criterion 5 (stack:ephemeral --with objectstore) cannot pass in this \
    sandbox: it denies AF_UNIX socket creation everywhere, and SeaweedFS 4.48 weed filer/s3 \
    always open a unix socket.\nNeeded: run criterion 5 with unix sockets allowed or outside the \
    sandbox.";

#[test]
fn a_long_block_reason_prints_whole_and_wrapped_under_its_label() {
    let mut stage = make_stage_summary("local-runtime", StageStatus::Blocked);
    stage.close_reason = Some(LONG_REASON.to_string());

    let output = guidance_output(stage, 60);

    let note: Vec<&str> = output
        .lines()
        .take_while(|line| !line.trim_start().starts_with("Once"))
        .collect();
    assert!(
        note.len() > 3,
        "the reason wraps onto several lines: {output}"
    );
    assert!(note[0].starts_with("    Note: blocked by the stage agent: Criterion 5"));
    for line in &note {
        assert!(line.chars().count() <= 60, "{line:?}");
    }
    for continuation in &note[1..] {
        assert!(continuation.starts_with("          "), "{continuation:?}");
    }
    let printed: Vec<&str> = note
        .iter()
        .flat_map(|line| line.split_whitespace())
        .collect();
    let expected = format!("Note: blocked by the stage agent: {LONG_REASON}");
    assert_eq!(printed, expected.split_whitespace().collect::<Vec<_>>());
}

#[test]
fn a_block_prints_the_note_then_the_retry_then_the_alternatives() {
    let mut stage = make_stage_summary("local-runtime", StageStatus::Blocked);
    stage.close_reason = Some("needs the unsandboxed run".to_string());

    let output = guidance_output(stage, 200);

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines[0],
        "    Note: blocked by the stage agent: needs the unsandboxed run"
    );
    assert!(lines[1].starts_with("    Once you have done what the reason asks"));
    assert_eq!(
        lines[2],
        "      loom stage retry local-runtime --context \"<what you did and what it showed>\""
    );
    assert!(lines[3].starts_with("    If the reason is an acceptance criterion"));
    assert!(lines[3].contains("find the criterion's index in the acceptance list"));
    assert!(
        lines[4].starts_with("      loom stage amend local-runtime --field acceptance --op delete")
    );
    assert!(lines[6].contains("--op replace"));
    assert!(lines[7].starts_with("    If you give up on the stage"));
    assert_eq!(lines[8], "      loom stage skip local-runtime");
    assert!(
        !output.contains("Run:"),
        "no plain retry is offered: {output}"
    );
}

#[test]
fn a_plain_command_keeps_its_run_line() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.failure_info = Some(FailureInfo {
        failure_type: FailureType::TestFailure,
        detected_at: Utc::now(),
        evidence: vec![],
    });

    assert_eq!(
        guidance_output(stage, 80),
        "    Run: loom stage retry server\n"
    );
}

#[path = "attention_acceptance_tests.rs"]
mod acceptance_tests;
