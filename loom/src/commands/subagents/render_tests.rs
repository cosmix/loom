use super::*;
use classify::DEFAULT_DONE_DEBOUNCE_SECS;

#[test]
fn list_on_unresolvable_dir_still_succeeds() {
    let result = list(
        None,
        Some(PathBuf::from("/nonexistent/subagents/dir")),
        false,
        DEFAULT_DONE_DEBOUNCE_SECS,
    );
    assert!(result.is_ok());
}

#[test]
fn non_forwarder_keeps_legacy_list_inputs_and_exit_zero() {
    let temp = tempfile::tempdir().unwrap();
    let content = serde_json::json!({
        "type": "assistant",
        "timestamp": (chrono::Utc::now() - chrono::Duration::minutes(4)).to_rfc3339(),
        "message": {"content": [{"type": "text", "text": "finished"}]},
    });
    std::fs::write(
        temp.path().join("agent-ordinary.jsonl"),
        content.to_string(),
    )
    .unwrap();

    let Gathered::Found(summaries) = gather(
        &None,
        &Some(temp.path().to_path_buf()),
        DEFAULT_DONE_DEBOUNCE_SECS,
        None,
        classify::resolve_subagent_ceiling(None),
    ) else {
        panic!("an explicit transcript directory must resolve");
    };
    assert_eq!(summaries[0].state, SubagentState::Done);
    assert!(serde_json::to_value(&summaries[0])
        .unwrap()
        .get("forward")
        .is_none());

    assert!(list(
        None,
        Some(temp.path().to_path_buf()),
        false,
        DEFAULT_DONE_DEBOUNCE_SECS
    )
    .is_ok());
}

#[test]
fn harvest_on_empty_dir_reports_nothing_without_erroring() {
    let result = harvest(
        None,
        None,
        Some(PathBuf::from("/nonexistent/subagents/dir")),
        DEFAULT_DONE_DEBOUNCE_SECS,
    );
    assert!(result.is_ok());
}

#[test]
fn harvest_emits_nothing_for_undebounced_text_only_entry() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "still narrating"}],
            },
        })
    );
    std::fs::write(temp.path().join("agent-x.jsonl"), content).unwrap();

    let Gathered::Found(summaries) = gather(
        &None,
        &Some(temp.path().to_path_buf()),
        DEFAULT_DONE_DEBOUNCE_SECS,
        None,
        classify::resolve_subagent_ceiling(None),
    ) else {
        panic!("an explicit --dir must always resolve");
    };
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].final_report.is_none());

    let result = harvest(
        None,
        None,
        Some(temp.path().to_path_buf()),
        DEFAULT_DONE_DEBOUNCE_SECS,
    );
    assert!(result.is_ok());
}

#[test]
fn tool_wait_idle_30_minutes_never_harvests_or_settles() {
    let temp = tempfile::tempdir().unwrap();
    let timestamp = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": timestamp,
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "name": "Bash", "input": {}}],
            },
        })
    );
    std::fs::write(temp.path().join("agent-x.jsonl"), content).unwrap();

    let Gathered::Found(summaries) = gather(
        &None,
        &Some(temp.path().to_path_buf()),
        DEFAULT_DONE_DEBOUNCE_SECS,
        None,
        classify::resolve_subagent_ceiling(None),
    ) else {
        panic!("an explicit --dir must always resolve");
    };
    assert_eq!(summaries[0].state, SubagentState::ToolWait);
    assert!(summaries[0].final_report.is_none());
    assert_eq!(
        forward::watch_outcome(&summaries),
        forward::WatchOutcome::Pending
    );
}
