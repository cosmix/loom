//! Recovery context helpers shared by the retry command.

use std::path::Path;

use crate::orchestrator::signals::{LastHeartbeatInfo, RecoveryReason, RecoverySignalContent};
use crate::orchestrator::{heartbeat_path, read_heartbeat};

/// Load last heartbeat information for a stage
pub(crate) fn load_last_heartbeat(work_dir: &Path, stage_id: &str) -> Option<LastHeartbeatInfo> {
    let hb_path = heartbeat_path(work_dir, stage_id);
    let heartbeat = read_heartbeat(&hb_path).ok()?;

    Some(LastHeartbeatInfo {
        timestamp: heartbeat.timestamp,
        context_tokens: heartbeat.context_tokens,
        last_tool: heartbeat.last_tool,
        activity: heartbeat.activity,
    })
}

/// Determine recovery reason from stage state
pub(crate) fn determine_recovery_reason(stage: &crate::models::stage::Stage) -> RecoveryReason {
    if let Some(ref reason) = stage.close_reason {
        let reason_lower = reason.to_lowercase();
        if reason_lower.contains("crash") || reason_lower.contains("orphan") {
            return RecoveryReason::Crash;
        }
        if reason_lower.contains("hung") || reason_lower.contains("heartbeat") {
            return RecoveryReason::Hung;
        }
        if reason_lower.contains("context") || reason_lower.contains("handoff") {
            return RecoveryReason::ContextExhaustion;
        }
    }

    // Default to manual recovery
    RecoveryReason::Manual
}

/// The close reason a recovery leaves on the requeued stage, with the
/// operator's `--context` when one was given.
pub(crate) fn recovery_close_reason(attempt: u32, context: Option<&str>) -> String {
    match context {
        Some(context) => format!("Recovery initiated (attempt #{attempt}): {context}"),
        None => format!("Recovery initiated (attempt #{attempt})"),
    }
}

/// Make the operator's `--context` the first thing the recovery session reads.
///
/// The context is how an operator answers a stage that blocked itself with
/// `loom stage block`: the recovery signal is the only message the next session
/// gets, and it takes the text verbatim, so a context left out of it is lost.
pub(crate) fn with_operator_answer(
    mut content: RecoverySignalContent,
    context: Option<&str>,
) -> RecoverySignalContent {
    if let Some(answer) = context {
        content.recovery_actions.insert(
            0,
            format!(
                "The operator answered the block that stopped the previous session. Their \
                 answer, verbatim:\n\n{answer}\n\nAct on it instead of blocking on the same \
                 need again."
            ),
        );
    }
    content
}

/// Generate a session ID for recovery
pub(crate) fn generate_recovery_session_id(stage_id: &str) -> String {
    let uuid_part = uuid::Uuid::new_v4().to_string();
    let short_uuid = &uuid_part[..8];
    let timestamp = chrono::Utc::now().timestamp();
    format!("recovery-{stage_id}-{short_uuid}-{timestamp}")
}

/// Find crash report for a session
pub(crate) fn find_crash_report(work_dir: &Path, session_id: &str) -> Option<std::path::PathBuf> {
    let crashes_dir = work_dir.join("crashes");
    if !crashes_dir.exists() {
        return None;
    }

    // Look for crash reports matching the session ID
    std::fs::read_dir(&crashes_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            name_str.contains(session_id) && name_str.ends_with(".md")
        })
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|e| e.path())
}

/// Extract resident context tokens from a close reason string.
pub(crate) fn extract_context_tokens(reason: &str) -> Option<u32> {
    let re = regex::Regex::new(r"(\d+)\s+tokens").ok()?;
    re.captures(reason)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_context_tokens() {
        assert_eq!(
            extract_context_tokens("Context at 75000 tokens"),
            Some(75_000)
        );
        assert_eq!(
            extract_context_tokens("context: 85000 tokens used"),
            Some(85_000)
        );
        assert_eq!(extract_context_tokens("no token count here"), None);
    }

    #[test]
    fn the_operators_context_reaches_the_recovery_signal_verbatim() {
        use crate::models::stage::{Stage, StageStatus};
        use crate::orchestrator::signals::generate_recovery_signal;

        let answer = "Ran TROOFOO_STACK=ephemeral bun run stack:ephemeral -- bun test.\n\
                      Exit code 0; 41 pass, 0 fail.\n\nNo log needed.";
        let temp = tempfile::TempDir::new().unwrap();
        let stage = Stage {
            id: "local-runtime".to_string(),
            name: "Local runtime".to_string(),
            status: StageStatus::Blocked,
            close_reason: Some("Adjudicator needs an unsandboxed run".to_string()),
            ..Stage::default()
        };
        // A reason that names no crash, hang or handoff is classified Manual.
        assert_eq!(determine_recovery_reason(&stage), RecoveryReason::Manual);
        let content = with_operator_answer(
            RecoverySignalContent::for_manual(
                "recovery-local-runtime-1".to_string(),
                "local-runtime".to_string(),
                "session-old".to_string(),
                1,
            ),
            Some(answer),
        );

        let path = generate_recovery_signal(&content, &stage, temp.path()).unwrap();

        let signal = std::fs::read_to_string(path).unwrap();
        assert!(signal.contains(answer), "{signal}");
        let answer_at = signal.find(answer).unwrap();
        let first_default = signal.find("Review any available handoff").unwrap();
        assert!(answer_at < first_default, "the answer comes first");
    }

    #[test]
    fn the_close_reason_carries_the_context_when_given() {
        assert_eq!(
            recovery_close_reason(2, Some("ran it, exit 0")),
            "Recovery initiated (attempt #2): ran it, exit 0"
        );
        assert_eq!(
            recovery_close_reason(1, None),
            "Recovery initiated (attempt #1)"
        );
    }

    #[test]
    fn no_context_leaves_the_recovery_actions_alone() {
        let content = RecoverySignalContent::for_manual(
            "recovery-s-1".to_string(),
            "s".to_string(),
            "session-old".to_string(),
            1,
        );
        let before = content.recovery_actions.clone();

        assert_eq!(with_operator_answer(content, None).recovery_actions, before);
    }

    #[test]
    fn test_determine_recovery_reason() {
        use crate::models::stage::{Stage, StageStatus};

        let mut stage = Stage {
            id: "test".to_string(),
            name: "Test".to_string(),
            status: StageStatus::Blocked,
            ..Stage::default()
        };

        // No reason - should be Manual
        assert_eq!(determine_recovery_reason(&stage), RecoveryReason::Manual);

        // Crash reason
        stage.close_reason = Some("Session crashed unexpectedly".to_string());
        assert_eq!(determine_recovery_reason(&stage), RecoveryReason::Crash);

        // Hung reason
        stage.close_reason = Some("No heartbeat for 5 minutes".to_string());
        assert_eq!(determine_recovery_reason(&stage), RecoveryReason::Hung);

        // Context exhaustion
        stage.close_reason = Some("Context limit reached, handoff created".to_string());
        assert_eq!(
            determine_recovery_reason(&stage),
            RecoveryReason::ContextExhaustion
        );
    }
}
