//! Desktop notification support for orchestrator events.
//!
//! Sends desktop notifications for events that need human attention,
//! using notify-send on Linux and osascript on macOS.

use crate::process::run_bounded_output;
use crate::utils::truncate;
use anyhow::{bail, Context, Result};
use std::process::Command;
use std::time::Duration;

const NOTIFY_SEND_TIMEOUT: Duration = Duration::from_secs(2);
const OSASCRIPT_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(3);

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

fn send_linux_notification(title: &str, body: &str) -> Result<()> {
    let mut command = Command::new("notify-send");
    command
        .arg("--urgency=critical")
        .arg("--app-name=loom")
        .arg(title)
        .arg(body);
    run_notification_command(
        &mut command,
        NOTIFY_SEND_TIMEOUT,
        "notify-send desktop notification",
        "notify-send",
    )
    .context("failed to run notify-send")
}

fn send_macos_notification(title: &str, body: &str) -> Result<()> {
    use crate::orchestrator::terminal::emulator::escape_applescript_string;

    let script = format!(
        r#"display notification "{}" with title "{}""#,
        escape_applescript_string(body),
        escape_applescript_string(title)
    );

    let mut command = Command::new("osascript");
    command.arg("-e").arg(&script);
    run_notification_command(
        &mut command,
        OSASCRIPT_NOTIFICATION_TIMEOUT,
        "osascript desktop notification",
        "osascript",
    )
    .context("failed to run osascript")
}

fn run_notification_command(
    command: &mut Command,
    timeout: Duration,
    operation: &str,
    program: &str,
) -> Result<()> {
    let output = run_bounded_output(command, timeout, operation)?;
    notification_command_succeeded(program, &output)
}

fn notification_command_succeeded(program: &str, output: &std::process::Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        bail!("{program} exited with {}", output.status);
    }
    bail!("{program} exited with {}: {stderr}", output.status)
}

/// Notify the user that a stage needs human review.
pub fn notify_needs_human_review(stage_id: &str, review_reason: Option<&str>) {
    let title = format!("loom: Stage '{}' needs review", stage_id);
    let body = review_reason
        .map(|r| truncate(r, 200))
        .unwrap_or_else(|| "A stage requires human review.".to_string());

    send_desktop_notification(&title, &body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessTimeoutError;
    use std::os::unix::process::ExitStatusExt;

    fn output(code: i32, stderr: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn notification_command_accepts_success() {
        notification_command_succeeded("notifier", &output(0, "")).unwrap();
    }

    #[test]
    fn notification_command_error_preserves_program_status_and_stderr() {
        let error = notification_command_succeeded("notifier", &output(7, "display unavailable\n"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("notifier"), "error: {error}");
        assert!(error.contains("exit status: 7"), "error: {error}");
        assert!(error.contains("display unavailable"), "error: {error}");
    }

    #[test]
    fn notification_command_timeout_is_bounded_and_typed() {
        let mut command = Command::new("sleep");
        command.arg("60");
        let timeout = Duration::from_millis(50);
        let started = std::time::Instant::now();

        let error = run_notification_command(&mut command, timeout, "test notification", "sleep")
            .expect_err("the notification command must time out");

        let timeout_error = error
            .downcast_ref::<ProcessTimeoutError>()
            .expect("timeout must remain machine-identifiable");
        assert_eq!(timeout_error.operation(), "test notification");
        assert_eq!(timeout_error.timeout(), timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
