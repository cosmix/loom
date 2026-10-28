//! Sanitising of delivered text and the checks on what the chat may send to the pane.

use std::fmt;

use super::attachments::MAX_ATTACHMENTS;
use crate::models::dispute::strip_control;

/// The most a delivered message may hold, in bytes, measured after `format_message` and
/// `sanitize` (section 12, "Request limits").
pub const MAX_DELIVERED_BYTES: usize = 100 * 1000;

/// The one slash command a chat message may start with.
const COMPACT: &str = "/compact";

/// Why the chat refused to send a message; `Display` is the text the browser shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputRefusal {
    SlashCommand,
    TooLong { bytes: usize, limit: usize },
    TooManyAttachments,
    Empty,
}

impl fmt::Display for InputRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlashCommand => {
                write!(
                    f,
                    "slash commands cannot be sent from the chat; only {COMPACT} is allowed"
                )
            }
            Self::TooLong { bytes, limit } => write!(
                f,
                "the message is {bytes} bytes with its attachments; the limit is {limit}"
            ),
            Self::TooManyAttachments => {
                write!(f, "a message carries at most {MAX_ATTACHMENTS} attachments")
            }
            Self::Empty => write!(f, "the message is empty"),
        }
    }
}

impl std::error::Error for InputRefusal {}

/// Remove every control character a pane could act on: C0 except `\n` and `\t`, DEL and C1.
/// `\r\n` becomes `\n` and a lone `\r` goes, since `\r` is itself a C0 control.
pub fn sanitize(text: &str) -> String {
    // `char::is_control` is exactly Unicode category Cc: C0, DEL (0x7F) and C1 (U+0080-U+009F).
    strip_control(text, &['\n', '\t'])
}

/// Check the user's own text as the pane will receive it (after [`sanitize`], so a control
/// character cannot hide a leading slash command): a slash command is refused unless it is
/// `/compact`. A leading U+FEFF is skipped with the whitespace: it is neither a control
/// character nor Rust whitespace, yet the browser's `String.prototype.trim` strips it.
pub fn check_send(text: &str) -> Result<(), InputRefusal> {
    let sanitized = sanitize(text);
    let trimmed = sanitized.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if !trimmed.starts_with('/') {
        return Ok(());
    }
    match trimmed.split_whitespace().next() {
        Some(COMPACT) => Ok(()),
        _ => Err(InputRefusal::SlashCommand),
    }
}

/// Check the text about to be pasted (already formatted and sanitised): at most
/// [`MAX_DELIVERED_BYTES`] bytes and not blank.
pub fn check_delivered(formatted: &str) -> Result<(), InputRefusal> {
    let bytes = formatted.len();
    if bytes > MAX_DELIVERED_BYTES {
        return Err(InputRefusal::TooLong {
            bytes,
            limit: MAX_DELIVERED_BYTES,
        });
    }
    if formatted.trim().is_empty() {
        return Err(InputRefusal::Empty);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_controls_and_keeps_text() {
        let raw = "a\x1b[201~b\u{9b}c\x7fd\x00e\u{85}f\r\ng\rh\ti\nKαλημέρα 中文";
        assert_eq!(sanitize(raw), "a[201~bcdef\ngh\ti\nKαλημέρα 中文");
    }

    #[test]
    fn sanitize_keeps_a_clean_text_unchanged() {
        let clean = "line one\n\tindented — ünïcode ✓";
        assert_eq!(sanitize(clean), clean);
    }

    #[test]
    fn check_send_allows_only_compact_among_slash_commands() {
        for refused in ["/clear", "  /exit", "\n/compactx", "/", "/model opus"] {
            assert_eq!(
                check_send(refused),
                Err(InputRefusal::SlashCommand),
                "{refused:?}"
            );
        }
        for accepted in [
            "/compact",
            "  /compact keep it",
            "a /clear later",
            "",
            "plain",
        ] {
            assert_eq!(check_send(accepted), Ok(()), "{accepted:?}");
        }
    }

    #[test]
    fn check_send_sees_the_text_the_pane_receives() {
        for refused in [
            "\u{1}/clear",
            "\x00/exit",
            "\x7f/permissions",
            "\u{9b}/model",
        ] {
            assert_eq!(
                check_send(refused),
                Err(InputRefusal::SlashCommand),
                "{refused:?}"
            );
        }
        assert_eq!(check_send("\u{1}/compact"), Ok(()));
    }

    #[test]
    fn check_send_skips_a_leading_byte_order_mark() {
        assert_eq!(
            check_send("\u{feff}/clear"),
            Err(InputRefusal::SlashCommand)
        );
        assert_eq!(
            check_send("\u{feff} \u{feff}/exit"),
            Err(InputRefusal::SlashCommand)
        );
        assert_eq!(check_send("\u{feff}/compact"), Ok(()));
    }

    #[test]
    fn check_delivered_counts_bytes_and_refuses_blank_text() {
        let at_limit = "a".repeat(MAX_DELIVERED_BYTES);
        assert_eq!(check_delivered(&at_limit), Ok(()));
        let over = format!("{at_limit}é");
        let refusal = InputRefusal::TooLong {
            bytes: MAX_DELIVERED_BYTES + 2,
            limit: MAX_DELIVERED_BYTES,
        };
        assert_eq!(check_delivered(&over), Err(refusal));
        assert_eq!(check_delivered(" \n\t"), Err(InputRefusal::Empty));
    }

    #[test]
    fn refusals_explain_themselves() {
        let too_long = InputRefusal::TooLong { bytes: 7, limit: 5 };
        assert!(too_long.to_string().contains('7') && too_long.to_string().contains('5'));
        assert!(InputRefusal::SlashCommand.to_string().contains("/compact"));
        assert!(InputRefusal::TooManyAttachments.to_string().contains("10"));
    }
}
