//! The last lines a session's terminal showed, for reports about a parked stage.
//!
//! A tmux session is read from its pane, a native session from its stderr log.
//! The result is text for a human (the park reason of a stalled stage); it is
//! never consulted for liveness.

use std::iter::Peekable;
use std::path::Path;
use std::str::Chars;

use crate::context::untrusted::flatten_char;
use crate::models::session::{Session, SessionBackendKind};

use super::{native, tmux};

/// The last `lines` non-empty lines of `session`'s terminal, control
/// characters stripped, or `None` when nothing is readable.
///
/// A tmux session is captured from its own server (`capture-pane`, bounded by
/// the tmux probe timeout). A native session's stderr log is read as
/// `lines * 4` raw lines first, because `read_log_tail` cuts raw lines, blank
/// ones included, and a log ending in blanks would otherwise yield fewer than
/// `lines` non-empty ones.
pub fn session_tail(session: &Session, work_dir: &Path, lines: usize) -> Option<String> {
    let raw = match session.backend {
        SessionBackendKind::Tmux => tmux::capture_pane_tail(session, lines),
        SessionBackendKind::Native => crate::orchestrator::spawner::read_log_tail(
            &native::stderr_log_path(work_dir, &session.id),
            lines.saturating_mul(4).max(lines),
        ),
    }?;
    normalise_tail(&raw, lines)
}

/// Strip ANSI CSI sequences and every control character but `\n` (a tab
/// becomes one space), trim trailing whitespace, drop blank lines, and keep
/// the last `lines`. `None` when `lines == 0` or nothing remains.
fn normalise_tail(raw: &str, lines: usize) -> Option<String> {
    if lines == 0 {
        return None;
    }
    let cleaned = strip_control(raw);
    let kept: Vec<&str> = cleaned
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = &kept[kept.len().saturating_sub(lines)..];
    (!tail.is_empty()).then(|| tail.join("\n"))
}

/// Drop CSI sequences whole (their parameters are printable text), then pass
/// every other character through [`flatten_char`], which turns controls, tabs,
/// bidi overrides and zero-width format characters into spaces: the pane text
/// is agent-controlled and is printed raw by `loom stage human-review`.
fn strip_control(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' if chars.peek() == Some(&'[') => {
                chars.next();
                skip_csi_body(&mut chars);
            }
            '\n' => out.push('\n'),
            c => out.push(flatten_char(c)),
        }
    }
    out
}

/// Consume the rest of a CSI sequence: parameter and intermediate bytes
/// (0x20-0x3f), then the final byte (0x40-0x7e). Any other character, a newline
/// included, means the sequence is malformed or cut off; it is left unconsumed
/// so a sequence with no final byte cannot swallow the lines after it.
fn skip_csi_body(chars: &mut Peekable<Chars<'_>>) {
    while let Some(&next) = chars.peek() {
        match next {
            '\u{20}'..='\u{3f}' => {
                chars.next();
            }
            '\u{40}'..='\u{7e}' => {
                chars.next();
                return;
            }
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn native_session() -> Session {
        let mut session = Session::new();
        session.backend = SessionBackendKind::Native;
        session
    }

    fn write_log(work: &Path, session: &Session, body: &str) {
        let path = native::stderr_log_path(work, &session.id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn normalise_tail_strips_control_characters_and_blank_lines() {
        let raw = "\u{1b}[31mred\u{1b}[0m  \r\n\n\tindented\u{7}\n   \nplain\u{1b}[2K";
        assert_eq!(
            normalise_tail(raw, 10).as_deref(),
            Some("red\n indented\nplain")
        );
    }

    #[test]
    fn an_unterminated_csi_sequence_does_not_swallow_the_lines_after_it() {
        let raw = "before \u{1b}[12;3\nafter\n\u{1b}[\nlast";
        assert_eq!(
            normalise_tail(raw, 10).as_deref(),
            Some("before\nafter\nlast")
        );
    }

    #[test]
    fn normalise_tail_removes_bidi_overrides_and_zero_width_characters() {
        let tail = normalise_tail("ok\u{202E}evil\u{200B}text\n", 5).unwrap();
        assert!(!tail.contains('\u{202E}'), "{tail:?}");
        assert!(!tail.contains('\u{200B}'), "{tail:?}");
        assert_eq!(tail, "ok evil text");
    }

    #[test]
    fn normalise_tail_keeps_the_last_n_lines() {
        let raw = "one\ntwo\n\nthree\nfour\n";
        assert_eq!(normalise_tail(raw, 2).as_deref(), Some("three\nfour"));
        assert_eq!(
            normalise_tail(raw, 10).as_deref(),
            Some("one\ntwo\nthree\nfour")
        );
    }

    #[test]
    fn normalise_tail_is_none_for_blank_input_and_zero_lines() {
        assert_eq!(normalise_tail("  \n\t\n\u{1b}[0m\n", 5), None);
        assert_eq!(normalise_tail("", 5), None);
        assert_eq!(normalise_tail("text\n", 0), None);
    }

    #[test]
    fn a_native_session_reads_its_stderr_log_tail() {
        let work = TempDir::new().unwrap();
        let session = native_session();
        write_log(
            work.path(),
            &session,
            "first\n\n\u{1b}[33msecond\u{1b}[0m\n\nthird\nfourth\n",
        );
        assert_eq!(
            session_tail(&session, work.path(), 3).as_deref(),
            Some("second\nthird\nfourth")
        );

        // A log ending in blank lines still yields `lines` non-empty ones.
        let padded = native_session();
        write_log(work.path(), &padded, "a\nb\nc\n\n\n\n\n");
        assert_eq!(
            session_tail(&padded, work.path(), 3).as_deref(),
            Some("a\nb\nc")
        );
    }

    #[test]
    fn a_tmux_session_without_a_server_has_no_tail() {
        let work = TempDir::new().unwrap();
        let mut session = Session::new();
        session.backend = SessionBackendKind::Tmux;
        session.assign_to_stage("stage-key".to_string());
        assert_eq!(session_tail(&session, work.path(), 40), None);
    }
}
