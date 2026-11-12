//! Validate the final done block before assistant text is shortened for display.

use serde_json::Value;

const MAX_BODY_BYTES: usize = 64 * 1024;
// Mirror web/src/plan/chat/closing.ts MAX_ENTRIES and MAX_ENTRY_CHARS: a fence the web would
// refuse must not end the chat. zod counts string length in UTF-16 code units.
const MAX_ENTRIES: usize = 50;
const MAX_ENTRY_UNITS: usize = 500;

pub(super) fn parse(text: &str) -> Result<bool, &'static str> {
    let Some(body) = final_body(text) else {
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

/// Walk fences so a done example inside another fenced block cannot end the chat.
fn final_body(text: &str) -> Option<&str> {
    let (mut open, mut offset) = (None, 0);
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
        if let Some((opened_with, opened_width, done, body_start)) = open {
            if marker == opened_with && width >= opened_width && info.is_empty() {
                if done && text[offset..].trim().is_empty() {
                    return Some(&text[body_start..start]);
                }
                open = None;
            }
        } else if marker != b'`' || !info.contains('`') {
            open = Some((marker, width, info == "loom-done", offset));
        }
    }
    None
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
    use super::parse;

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
}
