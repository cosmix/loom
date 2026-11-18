//! Sanitising of delivered text and the checks on what the chat may send to the pane.

use std::fmt;

use super::attachments::{unwrap_pasted, MAX_ATTACHMENTS};
use super::commands::{CatalogueState, CommandClass, CommandTable};
use crate::models::dispute::strip_control;

/// The most a delivered message may hold, in bytes, measured after `format_message` and
/// `sanitize` (section 12, "Request limits").
pub const MAX_DELIVERED_BYTES: usize = 100 * 1000;

/// The longest slash message the chat sends, in characters.
pub const MAX_SLASH_LEN: usize = 300;

/// The gate's refusals of a name the table does not list, by catalogue state.
const UNLISTED: &str = "Not in this session's command list";
const LOADING: &str = "Commands are still loading";
const UNAVAILABLE: &str = "This session's command list is unavailable";
/// A terminal-only row without a reason of its own.
const UNVERIFIED: &str = "Not verified in the planner";
const TOO_LONG: &str = "Too long for a command";
const ALONE: &str = "Send a command alone, on one line";

/// Why the chat refused to send a message; `Display` is the text the browser shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputRefusal {
    /// A slash message the gate refuses, with its reason.
    Command(String),
    TooLong {
        bytes: usize,
        limit: usize,
    },
    TooManyAttachments,
    Empty,
}

impl fmt::Display for InputRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(reason) => f.write_str(reason),
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

/// The key a sent message and the record of its delivery are paired by: [`sanitize`]d, without
/// Claude Code's paste wrappers wherever they sit, and with every whitespace run collapsed to one
/// space, since Claude Code may move a wrapper or trim the edges of a prompt. Both sides of a
/// comparison must go through it.
pub fn match_key(text: &str) -> String {
    let sanitized = sanitize(text);
    let unwrapped = unwrap_pasted(&sanitized);
    unwrapped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text of a message that starts with `/` once leading whitespace and U+FEFF are skipped
/// (the browser's `String.prototype.trim` strips U+FEFF, which is neither a control character
/// nor Rust whitespace). `sanitized` has had its control characters removed.
fn slash_text(sanitized: &str) -> Option<&str> {
    let lead = sanitized.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    lead.starts_with('/').then_some(lead)
}

/// `(name, args)` of a slash message, the name without its slash and the args trimmed; `None`
/// for a message that is no slash command. `sanitized` has had its control characters removed.
pub fn slash_line(sanitized: &str) -> Option<(&str, &str)> {
    let line = slash_text(sanitized)?.strip_prefix('/')?;
    let (name, args) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    Some((name, args.trim()))
}

/// Why `table` refuses `/name`, in the gate's words; `None` when it may be sent. A row decides
/// by its class (`Run` and `RunWithHandling` are sent, `TerminalOnly` is not); a name the table
/// has no row for is refused by the state of the catalogue.
pub fn class_refusal(table: &CommandTable, name: &str) -> Option<String> {
    match table.lookup(name) {
        Some(entry) if entry.class == CommandClass::TerminalOnly => {
            Some(entry.reason.as_deref().unwrap_or(UNVERIFIED).to_owned())
        }
        Some(_) => None,
        None => Some(
            match table.state() {
                CatalogueState::Ready { .. } => UNLISTED,
                CatalogueState::Loading => LOADING,
                CatalogueState::Unavailable { .. } => UNAVAILABLE,
            }
            .to_owned(),
        ),
    }
}

/// Check the user's own text as the pane receives it (after [`sanitize`], so a control
/// character cannot hide a leading slash command). A message that does not start with `/` is
/// accepted. A slash message must be one line of at most [`MAX_SLASH_LEN`] characters whose
/// first word `table` allows ([`class_refusal`]).
pub fn check_send(text: &str, table: &CommandTable) -> Result<(), InputRefusal> {
    let sanitized = sanitize(text);
    let Some(line) = slash_text(&sanitized).map(str::trim_end) else {
        return Ok(());
    };
    let refuse = |reason: &str| Err(InputRefusal::Command(reason.to_owned()));
    if line.contains('\n') {
        return refuse(ALONE);
    }
    if line.chars().count() > MAX_SLASH_LEN {
        return refuse(TOO_LONG);
    }
    let (name, _) = slash_line(line).unwrap_or_default();
    class_refusal(table, name).map_or(Ok(()), |reason| refuse(&reason))
}

/// A slash message carries no attachments. `sanitized` has had its control characters removed.
pub fn check_alone(sanitized: &str, attachments: usize) -> Result<(), InputRefusal> {
    if attachments > 0 && slash_line(sanitized).is_some() {
        return Err(InputRefusal::Command(ALONE.to_owned()));
    }
    Ok(())
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
    use serde_json::json;

    use super::*;

    const OUTSIDE: &str = "Changes the session outside the planner";

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

    fn ready() -> CommandTable {
        let rows = [
            json!({"name": "simplify", "description": "Tidy", "argumentHint": ""}),
            json!({"name": "loom-plan:args", "description": "(loom-plan) Echo", "aliases": ["args"]}),
        ];
        CommandTable::from_initialize(&rows, 1)
    }

    fn refused(reason: &str) -> Result<(), InputRefusal> {
        Err(InputRefusal::Command(reason.to_owned()))
    }

    #[test]
    fn check_send_decides_a_native_command_by_its_class() {
        let (ready, loading) = (ready(), CommandTable::loading());
        for table in [&ready, &loading] {
            assert_eq!(check_send("/compact", table), Ok(()));
            assert_eq!(check_send("  /compact keep it", table), Ok(()));
            assert_eq!(check_send("/context", table), Ok(()));
            let dialog = refused("Opens a terminal dialog");
            assert_eq!(check_send("/model opus", table), dialog);
            assert_eq!(check_send("/clear", table), refused(OUTSIDE));
            assert_eq!(check_send("  /exit", table), refused(OUTSIDE));
        }
    }

    #[test]
    fn check_send_decides_a_listed_name_by_the_catalogue_state() {
        let ready = ready();
        assert_eq!(check_send("/simplify", &ready), Ok(()));
        assert_eq!(check_send("/args x", &ready), Ok(()));
        assert_eq!(check_send("/compactx", &ready), refused(UNLISTED));
        assert_eq!(check_send("\n/compactx", &ready), refused(UNLISTED));
        assert_eq!(check_send("/", &ready), refused(UNLISTED));
        let loading = CommandTable::loading();
        assert_eq!(check_send("/simplify", &loading), refused(LOADING));
        let unavailable = CommandTable::unavailable("gone");
        assert_eq!(check_send("/simplify", &unavailable), refused(UNAVAILABLE));
    }

    #[test]
    fn check_send_accepts_text_that_does_not_start_with_a_slash() {
        let table = CommandTable::unavailable("gone");
        for accepted in ["a /clear later", "", "plain", "line\n/clear"] {
            assert_eq!(check_send(accepted, &table), Ok(()), "{accepted:?}");
        }
    }

    #[test]
    fn check_send_sees_the_text_the_pane_receives() {
        let table = ready();
        for hidden in ["\u{1}/clear", "\x00/exit", "\u{feff}/clear", "\u{85}/clear"] {
            assert_eq!(check_send(hidden, &table), refused(OUTSIDE), "{hidden:?}");
        }
        assert_eq!(
            check_send("\u{feff} \u{feff}/exit", &table),
            refused(OUTSIDE)
        );
        assert!(check_send("\x7f/permissions", &table).is_err());
        assert_eq!(check_send("\u{1}/compact", &table), Ok(()));
        assert_eq!(check_send("\u{feff}/compact", &table), Ok(()));
    }

    #[test]
    fn check_send_bounds_a_slash_message() {
        let table = ready();
        let line = |len: usize| format!("/simplify {}", "x".repeat(len - "/simplify ".len()));
        assert_eq!(check_send(&line(MAX_SLASH_LEN), &table), Ok(()));
        assert_eq!(
            check_send(&line(MAX_SLASH_LEN + 1), &table),
            refused(TOO_LONG)
        );
        assert_eq!(check_send("/simplify a\nb", &table), refused(ALONE));
        assert_eq!(check_send("/simplify a\n  ", &table), Ok(()));
        let long_multibyte = format!("/simplify {}", "é".repeat(MAX_SLASH_LEN - 10));
        assert_eq!(check_send(&long_multibyte, &table), Ok(()));
    }

    #[test]
    fn a_slash_message_takes_no_attachments() {
        assert_eq!(check_alone("/simplify", 0), Ok(()));
        assert_eq!(check_alone("plain", 2), Ok(()));
        assert_eq!(check_alone(" /simplify", 1), refused(ALONE));
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
        let reason = "Opens a terminal dialog";
        assert_eq!(InputRefusal::Command(reason.into()).to_string(), reason);
        assert!(InputRefusal::TooManyAttachments.to_string().contains("10"));
    }
}
