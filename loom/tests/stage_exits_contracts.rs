//! Contracts for stage stage-exits: a stage agent's block reason travels in
//! `StageSummary::close_reason` through the daemon's JSON frame and reaches the
//! attention panel as a note, while a crash-blocked stage stays automatic.
//!
//! The surface is `loom::commands::status::data::StageSummary`,
//! `loom::commands::status::render::attention_entries` and
//! `loom::models::failure::{FailureInfo, FailureType}`.

use loom::commands::status::data::{ActivityStatus, StageSummary, StageType};
use loom::commands::status::render::attention_entries;
use loom::models::failure::{FailureInfo, FailureType};
use loom::models::stage::StageStatus;

fn blocked_summary(id: &str, info: Option<FailureInfo>, reason: Option<String>) -> StageSummary {
    StageSummary {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        summary: None,
        status: StageStatus::Blocked,
        stage_type: StageType::Standard,
        dependencies: vec![],
        context_tokens: None,
        elapsed_secs: None,
        execution_secs: None,
        base_branch: None,
        base_merged_from: vec![],
        failure_info: info,
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
        close_reason: reason,
    }
}

/// Serialize and deserialize, as a TUI does with a daemon frame.
fn over_the_wire(stage: &StageSummary) -> StageSummary {
    let frame = serde_json::to_string(stage).expect("serialize StageSummary");
    serde_json::from_str(&frame).expect("deserialize StageSummary")
}

#[test]
fn agent_block_reason_reaches_attention_over_the_wire() {
    let reason = "needs registry.npmjs.org for bunx vitest";
    let stage = blocked_summary("s1", None, Some(reason.to_string()));

    let value = serde_json::to_value(&stage).expect("serialize StageSummary");
    assert_eq!(
        value.get("close_reason").and_then(|v| v.as_str()),
        Some(reason),
        "close_reason must be serialized under the key \"close_reason\"",
    );

    let received = over_the_wire(&stage);
    assert_eq!(received.close_reason.as_deref(), Some(reason));

    let entries = attention_entries(&[received]);
    assert_eq!(
        entries.len(),
        1,
        "one attention entry for one blocked stage"
    );
    let entry = &entries[0];
    assert_eq!(entry.id, "s1");
    assert_eq!(entry.label, "BLOCKED");
    assert_eq!(
        entry.note.as_deref(),
        Some("blocked: needs registry.npmjs.org for bunx vitest"),
    );
    assert_eq!(entry.command.as_deref(), Some("loom stage retry s1"));
    assert!(!entry.automatic, "an agent block needs the operator");
}

#[test]
fn crash_blocked_stage_is_not_an_agent_block() {
    let crash = FailureInfo {
        failure_type: FailureType::SessionCrash,
        detected_at: chrono::Utc::now(),
        evidence: vec!["boom".to_string()],
    };
    let stage = blocked_summary("s2", Some(crash), Some("Session crashed".to_string()));

    let received = over_the_wire(&stage);
    assert_eq!(received.close_reason.as_deref(), Some("Session crashed"));

    let entries = attention_entries(&[received]);
    assert_eq!(
        entries.len(),
        1,
        "one attention entry for one blocked stage"
    );
    let entry = &entries[0];
    assert_eq!(entry.id, "s2");
    assert!(
        entry.automatic,
        "a crash-blocked stage with retries left is loom's to retry",
    );
    assert!(
        !entry
            .note
            .as_deref()
            .is_some_and(|note| note.starts_with("blocked: ")),
        "a crash is not an agent block, got note {:?}",
        entry.note,
    );
}
