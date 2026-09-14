use super::*;
use classify::DEFAULT_DONE_DEBOUNCE_SECS;
use serial_test::serial;
use std::env;

/// Restores cwd on drop, even on panic. Needed because `list`/`harvest`
/// resolve their work dir by walking up from the process cwd, which
/// would otherwise adopt this checkout's own live `.loom/work`.
struct CwdGuard {
    original_dir: PathBuf,
}

impl CwdGuard {
    fn new() -> Self {
        Self {
            original_dir: env::current_dir().unwrap(),
        }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        env::set_current_dir(&self.original_dir).unwrap();
    }
}

/// Isolate the process cwd inside a fresh tempdir for the test's
/// duration, restoring the original cwd on drop. Returned as
/// `(CwdGuard, TempDir)`: declaration order drops the `TempDir` first,
/// which is safe since restoring cwd only needs the ORIGINAL directory
/// to still exist, not the one being left.
fn isolate_cwd() -> (CwdGuard, tempfile::TempDir) {
    let guard = CwdGuard::new();
    let isolated = tempfile::tempdir().unwrap();
    env::set_current_dir(isolated.path()).unwrap();
    (guard, isolated)
}

#[test]
#[serial]
fn list_on_unresolvable_dir_still_succeeds() {
    let (_cwd_guard, _isolated) = isolate_cwd();

    let result = list(
        None,
        Some(PathBuf::from("/nonexistent/subagents/dir")),
        false,
        DEFAULT_DONE_DEBOUNCE_SECS,
    );
    assert!(result.is_ok());
}

#[test]
#[serial]
fn non_forwarder_keeps_legacy_list_inputs_and_exit_zero() {
    let (_cwd_guard, _isolated) = isolate_cwd();

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
#[serial]
fn harvest_on_empty_dir_reports_nothing_without_erroring() {
    let (_cwd_guard, _isolated) = isolate_cwd();

    let result = harvest(
        None,
        None,
        Some(PathBuf::from("/nonexistent/subagents/dir")),
        DEFAULT_DONE_DEBOUNCE_SECS,
    );
    assert!(result.is_ok());
}

#[test]
#[serial]
fn harvest_emits_nothing_for_undebounced_text_only_entry() {
    let (_cwd_guard, _isolated) = isolate_cwd();

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
}

#[test]
fn harvest_terminal_failure_evidence_keeps_agent_state_and_reason() {
    let mut summary = super::super::summary::empty("agent-x".into(), 0, None);
    summary.state = SubagentState::Cancelled;
    summary.terminal_reason = Some("operator cancelled".into());

    assert_eq!(
        terminal_failure_evidence(&summary).as_deref(),
        Some("terminal failure evidence: agent=agent-x state=cancelled reason=operator cancelled")
    );
}
