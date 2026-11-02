//! The guidance of a stopped stage: a deliberate block, a crash, a setup
//! failure, and failed acceptance, each with the command that fits and what the
//! operator must do before it.
use super::*;

#[test]
fn a_blocked_stage_the_daemon_does_not_retry_gets_the_retry_command() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.failure_info = failure(FailureType::TestFailure);

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (Some("loom stage retry server"), None, false)
    );
}

const ANSWER: &str = "loom stage retry server --context \"<what you did and what it showed>\"";

#[test]
fn an_agent_block_shows_its_reason_and_asks_for_an_answer_in_the_retry() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("the schema migration is missing".to_string());
    let received: StageSummary =
        serde_json::from_str(&serde_json::to_string(&stage).unwrap()).unwrap();

    let entry = entry_for(received);

    assert_eq!(entry.label, "BLOCKED");
    assert_eq!(
        guidance(&entry),
        (
            Some(ANSWER),
            Some("blocked by the stage agent: the schema migration is missing"),
            false
        )
    );
    assert_eq!(
        entry.command_when.as_deref(),
        Some("you have done what the reason asks, retry with your answer (the next session receives it)")
    );
}

#[test]
fn an_agent_block_offers_to_amend_a_criterion_or_to_skip() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("criterion 5 cannot run here".to_string());

    let entry = entry_for(stage);

    let offered: Vec<&str> = entry
        .alternatives
        .iter()
        .map(|alternative| alternative.command.as_str())
        .collect();
    assert_eq!(
        offered,
        [
            "loom stage amend server --field acceptance --op delete --index <n>",
            "loom stage amend server --field acceptance --op replace --index <n> --value \"<new command>\"",
            "loom stage skip server",
        ]
    );
    assert!(entry.alternatives[0]
        .when
        .contains("find the criterion's index in the acceptance list"));
    assert!(entry.alternatives[2]
        .when
        .contains("dependents stay blocked"));
}

#[test]
fn a_review_rejection_names_the_operator_and_offers_no_amend() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("the output is wrong".to_string());
    stage.review_reason = Some("the output is wrong".to_string());

    let entry = entry_for(stage);

    assert_eq!(
        entry.note.as_deref(),
        Some("blocked by the operator (rejected in human review): the output is wrong")
    );
    let offered: Vec<&str> = entry
        .alternatives
        .iter()
        .map(|alternative| alternative.command.as_str())
        .collect();
    assert_eq!(offered, ["loom stage skip server"]);
}

#[test]
fn a_long_multi_line_reason_is_kept_whole_in_the_note() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    let reason = format!(
        "Needs an unsandboxed run only the operator can do. From the worktree root run: \
         TROOFOO_STACK=ephemeral bun run stack:ephemeral --with objectstore -- bun test \
         packages/storage; report exit code and test summary.\nOr allow AF_UNIX. {}",
        "More detail. ".repeat(40)
    );
    stage.close_reason = Some(reason.clone());

    let entry = entry_for(stage);

    assert_eq!(
        entry.note.as_deref(),
        Some(format!("blocked by the stage agent: {reason}").as_str())
    );
}

#[test]
fn an_agent_block_at_the_retry_limit_keeps_the_forced_retry() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("needs a decision".to_string());
    stage.retry_count = 3;

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (
            Some("loom stage retry server --force --context \"<what you did and what it showed>\""),
            Some("blocked by the stage agent: needs a decision"),
            false
        )
    );
}

#[test]
fn a_blocked_stage_names_its_terminal_history_for_text_renderers_only() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("needs a decision".to_string());
    stage.terminal_history_session = Some("session-ab12cd34-1700000000".to_string());
    let mut crashed = make_stage_summary("worker", StageStatus::Blocked);
    crashed.failure_info = failure(FailureType::TestFailure);
    crashed.terminal_history_session = Some("session-ef56ab78-1700000001".to_string());

    let entries = [stage, crashed].map(entry_for);

    // The web wire carries the note alone: a browser links the history.
    assert_eq!(
        entries[0].note.as_deref(),
        Some("blocked by the stage agent: needs a decision")
    );
    assert_eq!(entries[1].note, None);
    assert!(entries[0].text_note().unwrap().ends_with(
        "\nterminal history of its last session: less -R \
         .loom/work/terminal-history/session-ab12cd34-1700000000.terminal"
    ));
    assert_eq!(
        entries[1].text_note().as_deref(),
        Some(
            "terminal history of its last session: less -R \
             .loom/work/terminal-history/session-ef56ab78-1700000001.terminal"
        )
    );
}

#[test]
fn a_stage_that_is_not_blocked_names_no_terminal_history() {
    let mut stage = make_stage_summary("server", StageStatus::NeedsHumanReview);
    stage.terminal_history_session = Some("session-ab12cd34-1700000000".to_string());

    assert_eq!(entry_for(stage).terminal_history, None);
}

#[test]
fn a_known_failing_criterion_gets_the_amend_commands_filled_in() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("criterion 14 cannot pass here".to_string());
    stage.block_detail.failing_criteria = vec![FailingCriterion {
        index: 14,
        command: "bash run-all.sh".to_string(),
        diagnostic: None,
    }];

    let entry = entry_for(stage);

    let offered: Vec<&str> = entry
        .alternatives
        .iter()
        .map(|alternative| alternative.command.as_str())
        .collect();
    assert_eq!(
        offered,
        [
            "loom stage amend server --field acceptance --op delete --index 14",
            "loom stage amend server --field acceptance --op replace --index 14 --value \"<new command>\"",
            "loom stage skip server",
        ]
    );
    assert!(entry.alternatives[0].when.starts_with("criterion 14 "));
    assert_eq!(entry.failing_criteria[0].index, 14);
}

#[test]
fn a_blocked_stage_reports_its_close_reason_never_a_stale_review_reason() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("criterion 14 cannot pass here".to_string());
    stage.review_reason = Some("The full BSD hook suite takes too long".to_string());

    let entry = entry_for(stage);

    assert_eq!(
        entry.reason.as_deref(),
        Some("criterion 14 cannot pass here")
    );
    assert_eq!(entry.review_reason, None);
}

#[test]
fn a_stage_in_review_reports_its_review_reason() {
    let mut stage = make_stage_summary("server", StageStatus::NeedsHumanReview);
    stage.review_reason = Some("disputed twice".to_string());

    let entry = entry_for(stage);

    assert_eq!(entry.reason.as_deref(), Some("disputed twice"));
    assert_eq!(entry.review_reason.as_deref(), Some("disputed twice"));
}

#[test]
fn a_setup_failure_says_to_fix_the_cause_before_retrying() {
    for failure_type in [
        FailureType::InfrastructureError,
        FailureType::SandboxSetupFailure,
        FailureType::StartupRefusal,
    ] {
        let mut stage = make_stage_summary("server", StageStatus::Blocked);
        stage.failure_info = failure(failure_type);

        let entry = entry_for(stage);

        assert_eq!(entry.command.as_deref(), Some("loom stage retry server"));
        assert_eq!(
            entry.command_when.as_deref(),
            Some("you have fixed the cause named in the evidence")
        );
        assert!(entry
            .note
            .as_deref()
            .is_some_and(|note| note.contains("blocks the stage the same way")));
    }
}

#[test]
fn a_test_failure_keeps_the_plain_retry() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.failure_info = failure(FailureType::TestFailure);

    let entry = entry_for(stage);

    assert_eq!(entry.command_when, None);
    assert!(entry.alternatives.is_empty());
}

#[test]
fn a_crash_block_does_not_claim_an_agent_block() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.failure_info = failure(FailureType::TestFailure);
    stage.close_reason = Some("left over from an earlier block".to_string());

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (Some("loom stage retry server"), None, false)
    );
}

#[test]
fn a_crash_at_the_retry_limit_needs_a_forced_retry() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.failure_info = failure(FailureType::SessionCrash);
    stage.retry_count = 3;

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (
            Some("loom stage retry server --force"),
            Some("retry limit reached (3/3)"),
            false
        )
    );
}

#[test]
fn failed_acceptance_at_the_retry_limit_needs_a_forced_retry() {
    let mut stage = make_stage_summary("client", StageStatus::CompletedWithFailures);
    stage.retry_count = 2;
    stage.max_retries = Some(2);

    let entry = entry_for(stage);

    assert_eq!(entry.label, "ACCEPTANCE FAILED");
    assert_eq!(
        guidance(&entry),
        (
            Some("loom stage retry client --force"),
            Some("retry limit reached (2/2)"),
            false
        )
    );
}

#[test]
fn an_agent_block_carries_the_criteria_it_offers_to_amend() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("criterion 1 cannot run here".to_string());
    stage.acceptance = vec!["cargo build".to_string(), "cargo test".to_string()];

    let entry = entry_for(stage);

    assert_eq!(entry.acceptance, ["cargo build", "cargo test"]);
}

#[test]
fn a_rejection_in_review_and_a_crash_carry_no_criteria() {
    let mut rejected = make_stage_summary("server", StageStatus::Blocked);
    rejected.close_reason = Some("the output is wrong".to_string());
    rejected.review_reason = rejected.close_reason.clone();
    rejected.acceptance = vec!["cargo build".to_string()];
    let mut crashed = make_stage_summary("server", StageStatus::Blocked);
    crashed.failure_info = failure(FailureType::TestFailure);
    crashed.acceptance = vec!["cargo build".to_string()];

    assert!(entry_for(rejected).acceptance.is_empty());
    assert!(entry_for(crashed).acceptance.is_empty());
}
