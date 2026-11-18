//! Validate the final done block before assistant text is shortened for display, and keep that
//! block in the shortened text.

use serde_json::Value;

use super::records::cap_chars;

const MAX_BODY_BYTES: usize = 64 * 1024;
// Mirror web/src/plan/chat/closing.ts MAX_ENTRIES and MAX_ENTRY_CHARS: a fence the web would
// refuse must not end the chat. zod counts string length in UTF-16 code units.
const MAX_ENTRIES: usize = 50;
const MAX_ENTRY_UNITS: usize = 500;

pub(super) fn parse(text: &str) -> Result<bool, &'static str> {
    let Some(Fence { body, .. }) = scan(text).done else {
        return Ok(false);
    };
    if body.len() > MAX_BODY_BYTES {
        return Err("loom-done body exceeds 64 KiB");
    }
    let value: Value = serde_json::from_str(body).map_err(|_| "invalid loom-done JSON")?;
    let strings = |key| {
        value
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.len() <= MAX_ENTRIES
                    && entries.iter().all(|entry| {
                        entry
                            .as_str()
                            .is_some_and(|text| text.encode_utf16().count() <= MAX_ENTRY_UNITS)
                    })
            })
    };
    if !value.is_object() || !strings("decisions") || !strings("questions") {
        return Err(
            "loom-done requires decisions and questions arrays of at most 50 strings of at most 500 characters",
        );
    }
    Ok(true)
}

/// The final done fence of a text and where a fence still open at its end begins (byte offsets).
struct Scan<'a> {
    done: Option<Fence<'a>>,
    open_from: Option<usize>,
}

struct Fence<'a> {
    /// Offset of the opening line.
    start: usize,
    body: &'a str,
}

#[derive(Clone, Copy)]
struct Open {
    marker: u8,
    width: usize,
    done: bool,
    start: usize,
    body_start: usize,
}

/// Walk fences so a done example inside another fenced block cannot end the chat.
fn scan(text: &str) -> Scan<'_> {
    let (mut open, mut offset) = (None::<Open>, 0);
    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\r', '\n']);
        let unindented = line.trim_start_matches(' ');
        if line.len() - unindented.len() > 3 {
            continue;
        }
        let Some((marker, width, info)) = fence_line(unindented) else {
            continue;
        };
        if let Some(opened) = open {
            if marker == opened.marker && width >= opened.width && info.is_empty() {
                if opened.done && text[offset..].trim().is_empty() {
                    let body = &text[opened.body_start..start];
                    let done = Some(Fence {
                        start: opened.start,
                        body,
                    });
                    return Scan {
                        done,
                        open_from: None,
                    };
                }
                open = None;
            }
        } else if marker != b'`' || !info.contains('`') {
            open = Some(Open {
                marker,
                width,
                done: info == "loom-done",
                start,
                body_start: offset,
            });
        }
    }
    Scan {
        done: None,
        open_from: open.map(|opened| opened.start),
    }
}

/// The markdown the web receives for an assistant text, at most `max` characters, and whether
/// the text ends the turn. The full text decides the end. When a valid final done fence would be
/// cut off by the cap, the item keeps it: the head of the text, a blank line, then the fence
/// verbatim, so the web reads the same fence the server did. A fence that cannot fit alone is no
/// end (the web could not read it).
pub(super) fn display(text: &str, max: usize) -> (String, Result<bool, &'static str>) {
    let done = parse(text);
    let start = scan(text)
        .done
        .filter(|_| done == Ok(true))
        .map(|fence| fence.start);
    match start {
        Some(start) if text.chars().count() > max => keep_fence(text, start, max),
        _ => (cap_chars(text, max), done),
    }
}

fn keep_fence(text: &str, start: usize, max: usize) -> (String, Result<bool, &'static str>) {
    let fence = text[start..].trim_end();
    let Some(room) = max.checked_sub(fence.chars().count() + 2) else {
        return (
            cap_chars(text, max),
            Err("loom-done fence exceeds the display limit"),
        );
    };
    let mut head = cap_chars(&text[..start], room);
    // A cut inside a fenced block would swallow the done fence as that block's content.
    if let Some(open) = scan(&head).open_from {
        head.truncate(open);
    }
    let head = head.trim_end();
    let markdown = if head.is_empty() {
        fence.to_owned()
    } else {
        format!("{head}\n\n{fence}")
    };
    (markdown, Ok(true))
}

fn fence_line(line: &str) -> Option<(u8, usize, &str)> {
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = line.bytes().take_while(|byte| *byte == marker).count();
    (width >= 3).then(|| (marker, width, line[width..].trim()))
}

#[cfg(test)]
mod tests {
    use super::{display, parse};

    const FENCE: &str = "```loom-done\n{\"decisions\":[\"Written\"],\"questions\":[]}\n```";

    #[test]
    fn valid_final_fence_and_crlf() {
        assert_eq!(parse(&format!("Finished.\n\n{FENCE}\n \t")), Ok(true));
        assert_eq!(parse(&FENCE.replace('\n', "\r\n")), Ok(true));
    }

    #[test]
    fn text_after_the_fence_is_not_done() {
        assert_eq!(parse(&format!("{FENCE}\nAnything else?")), Ok(false));
    }

    #[test]
    fn the_info_string_is_exact() {
        for info in ["loom-done-later", "loom-done x", "loom-closing"] {
            assert_eq!(parse(&FENCE.replace("loom-done", info)), Ok(false));
        }
    }

    #[test]
    fn invalid_json_and_wrong_shapes_are_refused() {
        for body in [
            "not JSON",
            "[]",
            "null",
            "{}",
            r#"{"decisions":[],"questions":[42]}"#,
            r#"{"decisions":"written","questions":[]}"#,
        ] {
            assert!(parse(&format!("```loom-done\n{body}\n```")).is_err());
        }
    }

    fn with_lists(decisions: &[String], questions: &[String]) -> String {
        let body = serde_json::json!({ "decisions": decisions, "questions": questions });
        format!("```loom-done\n{body}\n```")
    }

    #[test]
    fn at_most_fifty_entries_per_list() {
        let entries = |count| vec!["x".to_owned(); count];
        assert_eq!(parse(&with_lists(&entries(50), &entries(50))), Ok(true));
        assert!(parse(&with_lists(&entries(51), &[])).is_err());
        assert!(parse(&with_lists(&[], &entries(51))).is_err());
    }

    #[test]
    fn entries_are_limited_in_utf16_units() {
        let one = |text: String| with_lists(&[text], &[]);
        assert_eq!(parse(&one("x".repeat(500))), Ok(true));
        assert!(parse(&one("x".repeat(501))).is_err());
        // A non-BMP char is two UTF-16 units, so 250 of them fill the limit.
        assert_eq!(parse(&one("😀".repeat(250))), Ok(true));
        assert!(parse(&one(format!("{}x", "😀".repeat(250)))).is_err());
        assert!(parse(&one("😀".repeat(251))).is_err());
    }

    #[test]
    fn oversized_body_is_refused_before_json_parsing() {
        // Invalid JSON distinguishes the size guard from the parser's error.
        let body = "x".repeat(super::MAX_BODY_BYTES + 1);
        assert_eq!(
            parse(&format!("```loom-done\n{body}\n```")),
            Err("loom-done body exceeds 64 KiB")
        );
    }

    #[test]
    fn quoted_fences_and_unclosed_blocks_are_not_done() {
        for text in [
            format!("````text\n{FENCE}\n````"),
            FENCE.lines().map(|line| format!("> {line}\n")).collect(),
            FENCE.trim_end_matches('`').to_owned(),
        ] {
            assert_eq!(parse(&text), Ok(false));
        }
    }

    const CAP: usize = 200;

    #[test]
    fn a_long_text_ending_in_the_fence_keeps_the_fence_in_the_item() {
        let text = format!("{}\n{FENCE}\n", "word ".repeat(100));
        let (markdown, done) = display(&text, CAP);
        assert_eq!(done, Ok(true));
        assert!(markdown.ends_with(FENCE), "{markdown}");
        assert!(markdown.chars().count() <= CAP);
        assert_eq!(parse(&markdown), Ok(true));
        assert!(markdown.starts_with("word word"));
    }

    #[test]
    fn a_cut_inside_an_open_block_drops_that_block_from_the_head() {
        let text = format!("Intro\n```text\n{}\n```\n{FENCE}", "line\n".repeat(80));
        let (markdown, done) = display(&text, CAP);
        assert_eq!(done, Ok(true));
        assert_eq!(markdown, format!("Intro\n\n{FENCE}"));
        assert_eq!(parse(&markdown), Ok(true));
    }

    #[test]
    fn a_fence_that_cannot_fit_is_no_end() {
        let text = format!("{}\n{FENCE}", "word ".repeat(100));
        let (markdown, done) = display(&text, FENCE.len() + 1);
        assert_eq!(done, Err("loom-done fence exceeds the display limit"));
        assert_eq!(markdown.chars().count(), FENCE.len() + 1);
    }

    #[test]
    fn a_long_example_fence_followed_by_text_is_cut_as_before() {
        let text = format!("{}\n{FENCE}\nStill working.", "word ".repeat(100));
        let (markdown, done) = display(&text, CAP);
        assert_eq!(done, Ok(false));
        assert_eq!(markdown, text.chars().take(CAP).collect::<String>());
    }

    #[test]
    fn a_short_text_is_shown_whole() {
        let text = format!("Done.\n{FENCE}\n");
        assert_eq!(display(&text, CAP), (text.clone(), Ok(true)));
    }
}
