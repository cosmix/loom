//! Claude Code's pasted-content wrapper. A pasted prompt is recorded (in the transcript and
//! the `UserPromptSubmit` hook) as `<pasted_content id="X">`, the pasted text, and a closing
//! `</pasted_content id="X">` repeating the id. Older records carry the text bare.

const OPEN_PREFIX: &str = "<pasted_content id=\"";
const CLOSE_PREFIX: &str = "</pasted_content id=\"";
const TAG_SUFFIX: &str = "\">";
const ID_MAX: usize = 32;

/// The pasted text inside the wrapper, or `delivered` unchanged. Only a message that is, after
/// its surrounding blank lines, exactly an open tag line, a body and a close tag line with the
/// same well-formed id is unwrapped; a wrapper amid typed text, a missing close or a mismatched
/// or malformed id leaves the message whole.
pub fn unwrap_pasted(delivered: &str) -> &str {
    pasted_body(delivered).unwrap_or(delivered)
}

fn pasted_body(delivered: &str) -> Option<&str> {
    let text = trim_blank_lines(delivered);
    let (open, rest) = text.split_once('\n')?;
    let (body, close) = rest.rsplit_once('\n')?;
    let id = tag_id(open, OPEN_PREFIX)?;
    (tag_id(close, CLOSE_PREFIX)? == id).then_some(body)
}

/// The id of a whole tag line `<prefix>X">`, when `X` is a valid id.
fn tag_id<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let id = line.strip_prefix(prefix)?.strip_suffix(TAG_SUFFIX)?;
    let valid = (1..=ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    valid.then_some(id)
}

/// `text` without its leading and trailing whitespace-only lines.
fn trim_blank_lines(text: &str) -> &str {
    let mut text = text;
    while let Some((line, rest)) = text.split_once('\n') {
        if !line.trim().is_empty() {
            break;
        }
        text = rest;
    }
    while let Some((rest, line)) = text.rsplit_once('\n') {
        if !line.trim().is_empty() {
            break;
        }
        text = rest;
    }
    text
}
