//! Desktop notification support for orchestrator events.
//!
//! Sends desktop notifications for events that need human attention,
//! using notify-send on Linux and osascript on macOS.

use std::process::Command;

/// Send a desktop notification.
///
/// Uses platform-appropriate notification tools:
/// - Linux: `notify-send`
/// - macOS: `osascript` with display notification
///
/// Failures are logged but never propagated - notifications are best-effort.
pub fn send_desktop_notification(title: &str, body: &str) {
    let result = if cfg!(target_os = "macos") {
        send_macos_notification(title, body)
    } else {
        send_linux_notification(title, body)
    };

    if let Err(e) = result {
        eprintln!("Desktop notification failed: {e}");
    }
}

fn send_linux_notification(title: &str, body: &str) -> Result<(), String> {
    Command::new("notify-send")
        .arg("--urgency=critical")
        .arg("--app-name=loom")
        .arg(title)
        .arg(body)
        .output()
        .map_err(|e| format!("notify-send failed: {e}"))
        .and_then(|output| {
            if output.status.success() {
                Ok(())
            } else {
                Err(format!("notify-send exited with: {}", output.status))
            }
        })
}

fn send_macos_notification(title: &str, body: &str) -> Result<(), String> {
    let script = format!(
        r#"display notification "{}" with title "{}""#,
        body.replace('"', r#"\""#),
        title.replace('"', r#"\""#)
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("osascript failed: {e}"))
        .and_then(|output| {
            if output.status.success() {
                Ok(())
            } else {
                Err(format!("osascript exited with: {}", output.status))
            }
        })
}

/// Notify the user that a stage needs human review.
pub fn notify_needs_human_review(stage_id: &str, review_reason: Option<&str>) {
    let title = format!("loom: Stage '{}' needs review", stage_id);
    let body = review_reason
        .map(|r| truncate_reason(r, 200))
        .unwrap_or_else(|| "A stage requires human review.".to_string());

    send_desktop_notification(&title, &body);
}

/// Truncate a reason string to max_len characters, adding ellipsis if needed.
fn truncate_reason(reason: &str, max_len: usize) -> String {
    if reason.len() <= max_len {
        reason.to_string()
    } else {
        format!("{}...", &reason[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_reason_short() {
        assert_eq!(truncate_reason("short", 200), "short");
    }

    #[test]
    fn test_truncate_reason_long() {
        let long = "a".repeat(300);
        let result = truncate_reason(&long, 200);
        assert_eq!(result.len(), 200);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_reason_exact() {
        let exact = "a".repeat(200);
        assert_eq!(truncate_reason(&exact, 200), exact);
    }
}
