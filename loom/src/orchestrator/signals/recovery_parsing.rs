//! Parsing recovery signal files.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};

use super::recovery_types::{LastHeartbeatInfo, RecoveryReason, RecoverySignalContent};

/// Read a recovery signal file
pub fn read_recovery_signal(
    work_dir: &Path,
    session_id: &str,
) -> Result<Option<RecoverySignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path)
        .with_context(|| format!("Failed to read recovery signal: {}", signal_path.display()))?;

    // Check if this is a recovery signal by looking for the recovery context section
    let Some(context) = section(&content, "## Recovery Context") else {
        return Ok(None);
    };

    // Parse basic information from the signal
    // Note: Field names are passed without colon - extract_field adds **: pattern
    let target = section(&content, "## Target").unwrap_or_default();
    let stage_id = extract_field(&target, "Stage")
        .filter(|id| !id.is_empty())
        .context("Recovery signal is missing stage_id")?
        .to_string();
    let previous_session_id = extract_field(&context, "Previous Session")
        .unwrap_or_default()
        .to_string();
    let recovery_attempt = extract_field(&context, "Recovery Attempt")
        .and_then(|s| s.trim_start_matches('#').parse().ok())
        .unwrap_or(1);

    let reason = parse_recovery_reason(&context);

    // Parse crash_report_path from "- **Crash Report**: {path}" line
    let crash_report_path = extract_field(&context, "Crash Report")
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(s.trim()));

    // Parse detected_at timestamp from "- **Detected At**: {timestamp}" line
    let detected_at = extract_field(&context, "Detected At")
        .and_then(|s| parse_timestamp(s.trim()))
        .unwrap_or_else(Utc::now);

    // Parse last heartbeat info from "### Last Known State" section
    let last_heartbeat = parse_last_heartbeat(&context);

    // Parse recovery actions from "### Recovery Actions" section
    let recovery_actions = parse_recovery_actions(&context);

    Ok(Some(RecoverySignalContent {
        session_id: session_id.to_string(),
        stage_id,
        previous_session_id,
        reason,
        detected_at,
        last_heartbeat,
        crash_report_path,
        recovery_actions,
        recovery_attempt,
    }))
}

fn parse_recovery_reason(context: &str) -> RecoveryReason {
    let reason_field = extract_field(context, "Reason").unwrap_or_default();
    if reason_field.starts_with("Session crashed") {
        RecoveryReason::Crash
    } else if reason_field.starts_with("Session hung") {
        RecoveryReason::Hung
    } else if reason_field.starts_with("Context exhaustion") {
        RecoveryReason::ContextExhaustion
    } else {
        RecoveryReason::Manual
    }
}

/// Parse a timestamp in "YYYY-MM-DD HH:MM:SS UTC" format
fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    // Expected format: "2025-01-24 15:30:45 UTC"
    let s = s.trim().trim_end_matches(" UTC");
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc())
}

/// Parse the "### Last Known State" section for heartbeat info
fn parse_last_heartbeat(content: &str) -> Option<LastHeartbeatInfo> {
    let content = section(content, "### Last Known State")?;

    // Find the section and extract fields
    let timestamp = extract_field(&content, "Timestamp").and_then(|s| parse_timestamp(s.trim()));

    // If no timestamp, we don't have valid heartbeat info
    let timestamp = timestamp?;

    let context_tokens = extract_field(&content, "Context Tokens").and_then(|s| {
        s.trim()
            .trim_end_matches("tokens")
            .trim()
            .parse::<u32>()
            .ok()
    });

    let last_tool = extract_field(&content, "Last Tool")
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_string());

    let activity = extract_field(&content, "Activity")
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_string());

    Some(LastHeartbeatInfo {
        timestamp,
        context_tokens,
        last_tool,
        activity,
    })
}

/// Parse the "### Recovery Actions" section
fn parse_recovery_actions(content: &str) -> Vec<String> {
    section(content, "### Recovery Actions")
        .map(|section| {
            super::helpers::signal_lines(&section)
                .filter(|(_, outside)| *outside)
                .filter_map(|(line, _)| super::helpers::numbered_item(line.trim()))
                .filter(|action| !action.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn section(content: &str, heading: &str) -> Option<String> {
    let level = heading.bytes().take_while(|b| *b == b'#').count();
    let mut found = false;
    let mut result = String::new();
    for (line, _, structural_heading) in super::helpers::signal_sections(content) {
        let trimmed = line.trim();
        if structural_heading.is_some() && trimmed == heading && !found {
            found = true;
            continue;
        }
        if found {
            if structural_heading.is_some_and(|(depth, _)| depth <= level) {
                break;
            }
            result.push_str(line);
            result.push('\n');
        }
    }
    found.then_some(result)
}

/// Extract a field value from markdown content.
///
/// Handles markdown bold formatting. Field should be just the name without
/// bold markers or colons. For example, use "Crash Report" not "**Crash Report**:".
///
/// The function looks for patterns like:
/// - `- **Field Name**: value`
/// - `- Field Name: value`
pub fn extract_field<'a>(content: &'a str, field: &str) -> Option<&'a str> {
    super::helpers::signal_lines(content)
        .filter(|(_, outside)| *outside)
        .find_map(|(line, _)| super::helpers::field_value(line, field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_and_fields_ignore_fenced_examples() {
        let actions = (1..=12)
            .map(|i| format!("{i}. action {i}\n"))
            .collect::<String>();
        let content =
            format!("### Recovery Actions\n```\n1. fake\n### Other\n```\n{actions}## End\n");
        assert_eq!(parse_recovery_actions(&content).len(), 12);
        assert_eq!(
            extract_field(
                "```\n- **Reason**: fake\n```\n- **Reason**: Manual",
                "Reason"
            ),
            Some("Manual")
        );
    }

    #[test]
    fn recovery_reason_comes_from_context_field() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("signals")).unwrap();
        std::fs::write(temp.path().join("signals/s.md"), "# Recovery Signal: s\n## Recovery Context\n- **Reason**: Manual\n## Target\n- **Stage**: mine\n## Assignment\nSession crashed, Session hung, Context exhaustion\n").unwrap();
        let parsed = read_recovery_signal(temp.path(), "s").unwrap().unwrap();
        assert!(matches!(parsed.reason, RecoveryReason::Manual));
    }

    #[test]
    fn recovery_actions_round_trip_through_formatter() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut original =
            RecoverySignalContent::for_crash("s".into(), "mine".into(), "old".into(), None, 1);
        original.reason = RecoveryReason::Manual;
        original.recovery_actions = (1..=12).map(|i| format!("action {i}")).collect();
        let stage = crate::models::stage::Stage {
            id: "mine".into(),
            description: Some("Session crashed; Session hung; Context exhaustion".into()),
            ..Default::default()
        };
        let rendered = super::super::recovery_format::format_recovery_signal(
            &original,
            &stage,
            &Default::default(),
        );
        std::fs::create_dir_all(temp.path().join("signals")).unwrap();
        std::fs::write(temp.path().join("signals/s.md"), rendered).unwrap();
        let parsed = read_recovery_signal(temp.path(), "s").unwrap().unwrap();
        assert_eq!(parsed.recovery_actions, original.recovery_actions);
        assert!(matches!(parsed.reason, RecoveryReason::Manual));
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2025-01-24 15:30:45 UTC");
        assert!(ts.is_some());
        let ts = ts.unwrap();
        assert_eq!(
            ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2025-01-24 15:30:45"
        );
    }

    #[test]
    fn test_parse_recovery_actions() {
        let content = r#"## Recovery Context

### Recovery Actions

1. Review the crash report for error details
2. Continue work from the last known state
3. If the issue persists, check for environmental problems

## Target
"#;
        let actions = parse_recovery_actions(content);
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0], "Review the crash report for error details");
        assert_eq!(actions[1], "Continue work from the last known state");
        assert_eq!(
            actions[2],
            "If the issue persists, check for environmental problems"
        );
    }

    #[test]
    fn test_parse_last_heartbeat() {
        let content = r#"## Recovery Context

### Last Known State

- **Timestamp**: 2025-01-24 14:30:00 UTC
- **Context Tokens**: 75000 tokens
- **Last Tool**: Read
- **Activity**: Reading file

### Recovery Actions
"#;
        let hb = parse_last_heartbeat(content);
        assert!(hb.is_some());
        let hb = hb.unwrap();
        assert_eq!(hb.context_tokens, Some(75_000));
        assert_eq!(hb.last_tool, Some("Read".to_string()));
        assert_eq!(hb.activity, Some("Reading file".to_string()));
    }

    #[test]
    fn test_extract_crash_report_path() {
        let content = "- **Crash Report**: /tmp/crash-report.md\n";
        let path = extract_field(content, "Crash Report");
        assert_eq!(path, Some("/tmp/crash-report.md"));
    }

    #[test]
    fn test_extract_field_bold_format() {
        // Test the bold format: "- **Field**: value"
        let content = "- **Stage**: my-stage-id\n";
        assert_eq!(extract_field(content, "Stage"), Some("my-stage-id"));
    }

    #[test]
    fn test_extract_field_plain_format() {
        // Test the plain format: "Field: value"
        let content = "Stage: my-stage-id\n";
        assert_eq!(extract_field(content, "Stage"), Some("my-stage-id"));
    }
}
