//! One test per row of the attention guidance table: what each entry tells
//! the operator to run, what it says, and whether loom is handling it.
use super::*;

#[test]
fn a_live_merge_resolver_is_automatic_with_no_command() {
    let mut stage = make_stage_summary("docs", StageStatus::MergeConflict);
    stage.merge_resolver_session = Some("session-abc".to_string());
    stage.merge_resolver_attempts = Some(1);

    let entry = entry_for(stage);

    assert_eq!(entry.label, "MERGE CONFLICT");
    assert_eq!(
        guidance(&entry),
        (
            None,
            Some("merge resolver session-abc is working (attempt 1 of 6)"),
            true
        )
    );
    assert!(!entry.has_human_review_choices);
}

#[test]
fn a_working_tmux_resolver_says_how_to_look_at_it() {
    let mut stage = make_stage_summary("docs", StageStatus::MergeConflict);
    stage.merge_resolver_session = Some("session-abc".to_string());
    stage.merge_resolver_attempts = Some(1);
    stage.session_backend = Some(SessionBackendKind::Tmux);

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (
            None,
            Some(
                "merge resolver session-abc is working (attempt 1 of 6); run loom attach docs to \
                 watch it, or open the stage's terminal in the web UI"
            ),
            true
        )
    );
}

#[test]
fn a_resolver_with_no_recent_tool_call_is_not_automatic_and_says_to_look_first() {
    let mut stage = make_stage_summary("docs", StageStatus::MergeConflict);
    stage.merge_resolver_session = Some("session-abc".to_string());
    stage.merge_resolver_attempts = Some(1);
    stage.session_backend = Some(SessionBackendKind::Native);
    stage.activity_status = ActivityStatus::Stale;
    stage.staleness_secs = Some(12 * 60 + 5);

    let entry = entry_for(stage);

    assert_eq!(entry.label, "MERGE CONFLICT");
    assert_eq!(
        guidance(&entry),
        (
            None,
            Some(
                "merge resolver session-abc made no tool call for 12 min and has not run loom \
                 stage merge docs --resolved: it may be inside one long command or may have \
                 stopped (attempt 1 of 6). Look first: it runs in the terminal window titled \
                 loom-merge-docs. Only if it is stuck, loom sessions kill session-abc ends it and \
                 the daemon starts the next attempt"
            ),
            false
        )
    );
}

#[test]
fn the_running_resolver_note_survives_the_daemon_wire() {
    // The daemon ships `StatusData` as JSON; the TUI and the web server build
    // attention from the copy they deserialize.
    let mut stage = make_stage_summary("docs", StageStatus::MergeConflict);
    stage.merge_resolver_session = Some("session-abc".to_string());
    stage.merge_resolver_attempts = Some(2);
    let received: StageSummary =
        serde_json::from_str(&serde_json::to_string(&stage).unwrap()).unwrap();

    let entry = entry_for(received);

    assert_eq!(
        guidance(&entry),
        (
            None,
            Some("merge resolver session-abc is working (attempt 2 of 6)"),
            true
        )
    );
}

#[test]
fn a_merge_error_without_a_resolver_waits_for_the_daemon() {
    let mut stage = make_stage_summary("docs", StageStatus::MergeBlocked);
    stage.merge_resolver_attempts = Some(2);

    let entry = entry_for(stage);

    assert_eq!(entry.label, "MERGE ERROR");
    assert_eq!(
        guidance(&entry),
        (
            None,
            Some("waiting for the daemon to start a merge resolver (2 of 6 attempts used)"),
            true
        )
    );
}

#[test]
fn a_crash_under_the_retry_limit_is_retried_automatically() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.failure_info = failure(FailureType::SessionCrash);
    stage.retry_count = 1;

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (None, Some("auto-retry 2 of 3 pending after a crash"), true)
    );
}

#[test]
fn a_timeout_names_itself_in_the_auto_retry_note() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.failure_info = failure(FailureType::Timeout);
    stage.max_retries = Some(5);

    let entry = entry_for(stage);

    assert_eq!(
        entry.note.as_deref(),
        Some("auto-retry 1 of 5 pending after a timeout")
    );
    assert!(entry.automatic);
}

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

#[test]
fn an_agent_block_shows_its_reason_and_retry() {
    let mut stage = make_stage_summary("server", StageStatus::Blocked);
    stage.close_reason = Some("the schema migration is missing".to_string());
    let received: StageSummary =
        serde_json::from_str(&serde_json::to_string(&stage).unwrap()).unwrap();

    let entry = entry_for(received);

    assert_eq!(entry.label, "BLOCKED");
    assert_eq!(
        guidance(&entry),
        (
            Some("loom stage retry server"),
            Some("blocked: the schema migration is missing"),
            false
        )
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
            Some("loom stage retry server --force"),
            Some("blocked: needs a decision"),
            false
        )
    );
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
fn needs_review_offers_the_choices_and_no_command() {
    let mut stage = make_stage_summary("verify", StageStatus::NeedsHumanReview);
    stage.review_reason = Some("criterion 3 disputed".to_string());

    let entry = entry_for(stage);

    assert_eq!(guidance(&entry), (None, None, false));
    assert!(entry.has_human_review_choices);
    assert_eq!(entry.review_reason.as_deref(), Some("criterion 3 disputed"));
}

#[test]
fn needs_input_points_at_the_stage_terminal() {
    let entry = entry_for(make_stage_summary("ask", StageStatus::WaitingForInput));

    assert_eq!(
        guidance(&entry),
        (
            None,
            Some("the agent is waiting on a question: answer it in the stage's terminal"),
            false
        )
    );
}

#[test]
fn adjudication_is_automatic() {
    let entry = entry_for(make_stage_summary("judge", StageStatus::NeedsAdjudication));

    assert_eq!(
        guidance(&entry),
        (
            None,
            Some("a judge session is ruling on the open disputes"),
            true
        )
    );
}

#[test]
fn an_executing_completion_blocker_is_automatic_prose() {
    let mut stage = make_stage_summary("writer", StageStatus::Executing);
    stage.completion_blocker = Some(completion_blocker(CompletionBlockerState::Pending));

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (None, Some("fix sandbox access, then retry"), true)
    );
    assert!(!entry.has_human_review_choices);
}

#[test]
fn a_parked_completion_blocker_takes_the_review_choices() {
    let mut stage = make_stage_summary("writer", StageStatus::NeedsHumanReview);
    stage.completion_blocker = Some(completion_blocker(CompletionBlockerState::Blocked));

    let entry = entry_for(stage);

    assert_eq!(
        guidance(&entry),
        (None, Some("fix sandbox access, then retry"), false)
    );
    assert!(entry.has_human_review_choices);
}

#[test]
fn an_unconfirmed_writer_offers_no_review_choices() {
    let mut stage = make_stage_summary("writer", StageStatus::NeedsHumanReview);
    stage.completion_blocker = Some(completion_blocker(CompletionBlockerState::OwnershipUnknown));

    let entry = entry_for(stage);

    assert!(!entry.automatic);
    assert!(!entry.has_human_review_choices);
}

#[test]
fn human_review_choices_are_full_commands() {
    let commands = human_review_choices("verify").map(|(command, _)| command);

    assert_eq!(
        commands,
        [
            "loom stage human-review verify --approve",
            "loom stage human-review verify --force-complete",
            "loom stage human-review verify --reject \"<reason>\"",
        ]
    );
}
