//! Parsing recovery signal files.

use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::Path;

use super::recovery_types::{RecoveryReason, RecoverySignalContent};

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
    if !content.contains("## Recovery Context") {
        return Ok(None);
    }

    // Parse basic information from the signal
    // Note: This is a simplified parser that extracts key fields
    let stage_id = extract_field(&content, "Stage:")
        .unwrap_or_default()
        .to_string();
    let previous_session_id = extract_field(&content, "Previous Session:")
        .unwrap_or_default()
        .to_string();
    let recovery_attempt = extract_field(&content, "Recovery Attempt:")
        .and_then(|s| s.trim_start_matches('#').parse().ok())
        .unwrap_or(1);

    let reason = if content.contains("Session crashed") {
        RecoveryReason::Crash
    } else if content.contains("Session hung") {
        RecoveryReason::Hung
    } else if content.contains("Context exhaustion") {
        RecoveryReason::ContextExhaustion
    } else {
        RecoveryReason::Manual
    };

    Ok(Some(RecoverySignalContent {
        session_id: session_id.to_string(),
        stage_id,
        previous_session_id,
        reason,
        detected_at: Utc::now(), // We don't parse this from the file
        last_heartbeat: None,
        crash_report_path: None,
        recovery_actions: vec![],
        recovery_attempt,
    }))
}

/// Extract a field value from markdown content
pub fn extract_field<'a>(content: &'a str, field: &str) -> Option<&'a str> {
    for line in content.lines() {
        if line.contains(field) {
            if let Some(value) = line.split(field).nth(1) {
                let value = value.trim().trim_start_matches("**").trim_end_matches("**");
                return Some(value);
            }
        }
    }
    None
}
