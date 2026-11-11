//! Claude Code's pasted-content wrapper. A pasted chunk is recorded (in the transcript and the
//! `UserPromptSubmit` hook) in place as `\n\n<pasted_content id="X">\n`, the chunk and
//! `\n</pasted_content id="X">\n`, repeating the id; text outside a chunk is kept as typed. A
//! message pasted whole is one chunk, and one pasted in parts is several, so the wrapper can sit
//! anywhere in the record. Short prompts are recorded bare.

const OPEN_PREFIX: &str = "<pasted_content id=\"";
const CLOSE_PREFIX: &str = "</pasted_content id=\"";
const TAG_SUFFIX: &str = "\">";
const ID_MAX: usize = 32;

/// The newlines Claude Code puts before an open tag line.
const LEAD_NEWLINES: usize = 2;

/// Byte range of one line of the message, without its `\n`.
#[derive(Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
}

/// `delivered` without its paste expansions. For every open tag line (`<pasted_content id="X">`,
/// a well-formed id) that has a later close tag line with the same id, the open line goes with
/// its newline and up to two newlines before it, and the close line with the newline before it
/// and the one after it, which leaves the chunk as Claude Code received it. A tag line without
/// its partner, a mismatched or malformed id, or a tag inside a line stays: a message with no
/// complete wrapper comes back unchanged.
///
/// Known limit: a wrapper the user typed as a complete pair of tag lines is removed too. The
/// sent side goes through the same removal when it builds its match key, so the pairing holds.
/// Revisit if Claude Code stops wrapping pastes.
pub fn unwrap_pasted(delivered: &str) -> std::borrow::Cow<'_, str> {
    let lines = line_spans(delivered);
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let Some(close) = matching_close(delivered, &lines, at) else {
            at += 1;
            continue;
        };
        cuts.push(open_cut(delivered, lines[at]));
        cuts.push(close_cut(delivered, lines[close]));
        at = close + 1;
    }
    if cuts.is_empty() {
        return delivered.into();
    }
    let mut kept = String::with_capacity(delivered.len());
    let mut pos = 0;
    for (from, to) in cuts {
        let from = from.max(pos);
        if to > from {
            kept.push_str(&delivered[pos..from]);
            pos = to;
        }
    }
    kept.push_str(&delivered[pos..]);
    kept.into()
}

fn line_spans(text: &str) -> Vec<Line> {
    let mut start = 0;
    text.split('\n')
        .map(|line| {
            let span = Line {
                start,
                end: start + line.len(),
            };
            start = span.end + 1;
            span
        })
        .collect()
}

/// The first line after the open tag line `at` that closes it, when `at` opens a wrapper.
fn matching_close(text: &str, lines: &[Line], at: usize) -> Option<usize> {
    let open = lines[at];
    let id = tag_id(&text[open.start..open.end], OPEN_PREFIX)?;
    (at + 1..lines.len())
        .find(|&j| tag_id(&text[lines[j].start..lines[j].end], CLOSE_PREFIX) == Some(id))
}

/// The open line, its newline and up to two newlines before it.
fn open_cut(text: &str, open: Line) -> (usize, usize) {
    let lead = text.as_bytes()[..open.start]
        .iter()
        .rev()
        .take(LEAD_NEWLINES)
        .take_while(|byte| **byte == b'\n')
        .count();
    (open.start - lead, (open.end + 1).min(text.len()))
}

/// The close line with the newline before it (a close line is never the first) and after it.
fn close_cut(text: &str, close: Line) -> (usize, usize) {
    (close.start - 1, (close.end + 1).min(text.len()))
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
