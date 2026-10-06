//! Desktop notification support for orchestrator events.
//!
//! Sends desktop notifications for events that need human attention,
//! using notify-send on Linux and osascript on macOS.

use crate::orchestrator::terminal::emulator::escape_applescript_string;
use crate::process::run_bounded_output;
use crate::utils::truncate;
use anyhow::{bail, Context, Result};
use std::process::Command;
use std::time::Duration;

const NOTIFY_SEND_TIMEOUT: Duration = Duration::from_secs(2);
const OSASCRIPT_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(3);

/// The longest review reason a notification body carries.
const REVIEW_REASON_CHARS: usize = 200;

/// One notifier invocation: the program, its arguments, and how long it may run.
struct Notifier {
    program: &'static str,
    args: Vec<String>,
    timeout: Duration,
}

/// `text` with the markup characters of a freedesktop notification body
/// escaped. Notification servers render `<a href=...>`, `<img>` and `<b>` in a
/// body, and a body can carry text an agent wrote.
fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The notifier for `os` (a `std::env::consts::OS` value): AppleScript's
/// `display notification` on macOS, `notify-send` everywhere else, whose body is
/// markup and is escaped.
fn notifier_for(os: &str, title: &str, body: &str) -> Notifier {
    if os == "macos" {
        let script = format!(
            r#"display notification "{}" with title "{}""#,
            escape_applescript_string(body),
            escape_applescript_string(title)
        );
        return Notifier {
            program: "osascript",
            args: vec!["-e".to_string(), script],
            timeout: OSASCRIPT_NOTIFICATION_TIMEOUT,
        };
    }
    Notifier {
        program: "notify-send",
        args: vec![
            "--urgency=critical".to_string(),
            "--app-name=loom".to_string(),
            title.to_string(),
            escape_markup(body),
        ],
        timeout: NOTIFY_SEND_TIMEOUT,
    }
}

/// Send a desktop notification.
///
/// Best-effort and never blocking: the notifier runs, bounded, on a thread of
/// its own, and a failure (including a host without one) is logged, never
/// propagated. A test build sends nothing, so no test run pops notifications
/// on the developer's desktop.
pub fn send_desktop_notification(title: &str, body: &str) {
    if cfg!(test) {
        return;
    }
    let notifier = notifier_for(std::env::consts::OS, title, body);
    let spawned = std::thread::Builder::new()
        .name("loom-notify".to_string())
        .spawn(move || {
            if let Err(e) = run_notifier(&notifier) {
                eprintln!("Desktop notification failed: {e}");
            }
        });
    if let Err(e) = spawned {
        eprintln!("Desktop notification failed: {e}");
    }
}

fn run_notifier(notifier: &Notifier) -> Result<()> {
    let mut command = Command::new(notifier.program);
    command.args(&notifier.args);
    run_notification_command(
        &mut command,
        notifier.timeout,
        &format!("{} desktop notification", notifier.program),
        notifier.program,
    )
    .with_context(|| format!("failed to run {}", notifier.program))
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

/// The body of a needs-review notification: the command that resolves it, then
/// `review_reason` cut to [`REVIEW_REASON_CHARS`]. The caller passes the one-line
/// headline of a reason, never its pane notes.
fn needs_review_body(stage_id: &str, review_reason: Option<&str>) -> String {
    let reason = review_reason
        .map(|r| truncate(r, REVIEW_REASON_CHARS))
        .unwrap_or_else(|| "A stage requires human review.".to_string());
    format!("Next: loom stage human-review {stage_id}\n{reason}")
}

/// Notify the user that a stage needs human review.
pub fn notify_needs_human_review(stage_id: &str, review_reason: Option<&str>) {
    let title = format!("loom: Stage '{}' needs review", stage_id);
    send_desktop_notification(&title, &needs_review_body(stage_id, review_reason));
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
    fn macos_notifies_through_applescript_with_its_strings_escaped() {
        let notifier = notifier_for("macos", r#"loom: "s1""#, "Next: x\nwhy");

        assert_eq!(notifier.program, "osascript");
        assert_eq!(
            notifier.args,
            [
                "-e",
                r#"display notification "Next: x\nwhy" with title "loom: \"s1\"""#,
            ]
        );
        assert_eq!(notifier.timeout, OSASCRIPT_NOTIFICATION_TIMEOUT);
    }

    #[test]
    fn every_other_os_notifies_through_notify_send_with_title_and_body_as_arguments() {
        let notifier = notifier_for("linux", "loom: Stage 's1' stalled", "Next: x");

        assert_eq!(notifier.program, "notify-send");
        assert_eq!(
            notifier.args,
            [
                "--urgency=critical",
                "--app-name=loom",
                "loom: Stage 's1' stalled",
                "Next: x",
            ]
        );
        assert_eq!(notifier.timeout, NOTIFY_SEND_TIMEOUT);
    }

    #[test]
    fn notify_send_bodies_are_escaped_so_no_markup_renders() {
        let notifier = notifier_for("linux", "loom: s1", r#"<a href="x">go</a> & <b>more</b>"#);

        assert_eq!(
            notifier.args[3],
            r#"&lt;a href="x"&gt;go&lt;/a&gt; &amp; &lt;b&gt;more&lt;/b&gt;"#
        );
    }

    #[test]
    fn a_review_notification_body_is_one_bounded_reason_without_raw_markup() {
        let reason = format!(r#"stalled: <a href="x">open</a> {}"#, "y".repeat(500));
        let body = needs_review_body("s1", Some(&reason));
        let sent = notifier_for("linux", "loom: s1", &body).args[3].clone();

        assert_eq!(body.lines().count(), 2, "{body}");
        assert!(
            body.starts_with("Next: loom stage human-review s1\n"),
            "{body}"
        );
        assert!(body.chars().count() <= 40 + REVIEW_REASON_CHARS, "{body}");
        assert!(!sent.contains('<') && !sent.contains('>'), "{sent}");
        assert!(sent.contains("&lt;a href="), "{sent}");
    }

    #[test]
    fn a_review_notification_without_a_reason_says_so() {
        assert_eq!(
            needs_review_body("s1", None),
            "Next: loom stage human-review s1\nA stage requires human review."
        );
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
