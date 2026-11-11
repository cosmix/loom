//! Reads the `<loom-attachments>` block back out of a delivered message.

use std::iter::Peekable;
use std::str::Split;

use super::format::{kind_from, locked_from, tag_line, CLOSE_TAG, OPEN_TAG};
use super::pasted::unwrap_pasted;
use super::stored::{extension, valid_id, MEDIA_TYPES};
use super::{Attachment, AttachmentKind, FileMeta, LockedTag};

/// The user's text and the attachments of a message Claude Code recorded (a hook prompt), after
/// removing its pasted-content wrappers ([`unwrap_pasted`]); see [`parse_unwrapped`].
pub fn parse_message(delivered: &str) -> (String, Vec<Attachment>) {
    parse_unwrapped(&unwrap_pasted(delivered))
}

/// [`parse_message`] of a message already unwrapped, so a body that itself looks like a
/// wrapper is not unwrapped twice, or of the server's own `format_message` output, which Claude
/// Code never wrapped, so a wrapper the user typed stays in the text. The block starts at the first line equal to
/// `<loom-attachments>` (preceded by a blank line) and runs to the last line
/// `</loom-attachments>`. A block that does not parse leaves the text whole with no
/// attachments; a message with no tag line only has its escaped tag lines restored.
pub fn parse_unwrapped(delivered: &str) -> (String, Vec<Attachment>) {
    let lines: Vec<&str> = delivered.split('\n').collect();
    let Some(open) = lines.iter().position(|line| *line == OPEN_TAG) else {
        return (unescape_text(&lines), Vec::new());
    };
    let blank_before = open >= 1 && lines[open - 1].is_empty();
    match parse_block(&lines[open + 1..]) {
        Some(attachments) if blank_before => (unescape_text(&lines[..open - 1]), attachments),
        _ => (delivered.to_owned(), Vec::new()),
    }
}

/// Whether `text` is a summary sentence id: `s-` and eight lowercase hex digits.
pub(super) fn is_sentence_id(text: &str) -> bool {
    let hex = text.strip_prefix("s-");
    hex.is_some_and(|hex| {
        hex.len() == 8
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Remove one backslash from every line that escapes a block tag.
fn unescape_text(lines: &[&str]) -> String {
    let restored: Vec<String> = lines
        .iter()
        .map(|line| match tag_line(line) {
            Some((offset, backslashes)) if backslashes > 0 => {
                format!("{}{}", &line[..offset], &line[offset + 1..])
            }
            _ => (*line).to_owned(),
        })
        .collect();
    restored.join("\n")
}

/// The entries between the open tag and the last close tag; `None` when there is no close tag,
/// text follows it, or there is no entry or one does not parse.
fn parse_block(body: &[&str]) -> Option<Vec<Attachment>> {
    let close = body.iter().rposition(|line| *line == CLOSE_TAG)?;
    if body[close + 1..].iter().any(|line| !line.trim().is_empty()) {
        return None;
    }
    let entries = parse_entries(&body[..close])?;
    (!entries.is_empty()).then_some(entries)
}

fn is_quoted(line: &str) -> bool {
    line.starts_with("> ") || line == ">"
}

fn parse_entries(lines: &[&str]) -> Option<Vec<Attachment>> {
    let mut entries: Vec<Attachment> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let mut entry = parse_header(lines[index], entries.len() + 1)?;
        index += 1;
        let start = index;
        while index < lines.len() && is_quoted(lines[index]) {
            index += 1;
        }
        let quoted: Vec<&str> = lines[start..index]
            .iter()
            .map(|line| line.strip_prefix("> ").unwrap_or(""))
            .collect();
        entry.quote = quoted.join("\n");
        entries.push(entry);
    }
    Some(entries)
}

/// `[n] <kind> <address>[ lines a-b][ rests-on a,b][ locked=<tag>][ changed-since-selected]`.
fn parse_header(line: &str, number: usize) -> Option<Attachment> {
    if let Some(rest) = line.strip_prefix(&format!("[{number}] file ")) {
        return parse_file_header(rest);
    }
    let mut tokens = line.split(' ').peekable();
    if tokens.next()? != format!("[{number}]") {
        return None;
    }
    let kind = kind_from(tokens.next()?).filter(|kind| *kind != AttachmentKind::File)?;
    let mut entry = with_address(kind, tokens.next()?)?;
    if tokens.next_if_eq(&"lines").is_some() {
        entry.lines = Some(parse_range(tokens.next()?)?);
    }
    if tokens.next_if_eq(&"rests-on").is_some() {
        entry.rests_on = tokens.next()?.split(',').map(str::to_owned).collect();
        if entry.rests_on.iter().any(String::is_empty) {
            return None;
        }
    }
    entry.locked = parse_locked(&mut tokens)?;
    entry.changed_since_selected = tokens.next_if_eq(&"changed-since-selected").is_some();
    tokens.next().is_none().then_some(entry)
}

fn parse_locked(tokens: &mut Peekable<Split<'_, char>>) -> Option<Option<LockedTag>> {
    let Some(token) = tokens.next_if(|token| token.starts_with("locked=")) else {
        return Some(None);
    };
    locked_from(token.strip_prefix("locked=")?).map(Some)
}

/// An entry holding the path and what the address after `#` says: the anchor, a stage id
/// (`stage:<id>`) and, for a summary, the sentence id.
fn with_address(kind: AttachmentKind, address: &str) -> Option<Attachment> {
    let (path, anchor) = match address.split_once('#') {
        Some((path, anchor)) if !anchor.is_empty() => (path, Some(anchor)),
        Some(_) => return None,
        None => (address, None),
    };
    if path.is_empty() {
        return None;
    }
    let sentence =
        anchor.filter(|anchor| kind == AttachmentKind::Summary && is_sentence_id(anchor));
    Some(Attachment {
        kind,
        path: path.to_owned(),
        anchor: anchor.map(str::to_owned),
        lines: None,
        stage: anchor
            .and_then(|anchor| anchor.strip_prefix("stage:"))
            .map(str::to_owned),
        summary_sentence: sentence.map(str::to_owned),
        rests_on: Vec::new(),
        quote: String::new(),
        changed_since_selected: false,
        locked: None,
        file: None,
    })
}

/// `<absolute path> (<media type>, <bytes> bytes) "<name>"` of a file chip. The path may hold
/// spaces, so each ` (` is tried as the start of the metadata; the first that parses wins. The
/// chip keeps no path (browsers never get it): its id is the stored file's name stem.
fn parse_file_header(rest: &str) -> Option<Attachment> {
    rest.match_indices(" (")
        .find_map(|(at, _)| file_chip(&rest[..at], &rest[at + 2..]))
}

fn file_chip(path: &str, tail: &str) -> Option<Attachment> {
    let (media_type, bytes, name) = parse_file_meta(tail)?;
    let stored = std::path::Path::new(path).file_name()?.to_str()?;
    let (id, ext) = stored.split_once('.')?;
    if !valid_id(id) || extension(&media_type) != Some(ext) {
        return None;
    }
    Some(Attachment {
        kind: AttachmentKind::File,
        path: String::new(),
        anchor: None,
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: String::new(),
        changed_since_selected: false,
        locked: None,
        file: Some(FileMeta {
            id: id.to_owned(),
            name,
            media_type,
            bytes,
        }),
    })
}

/// `<media type>, <bytes> bytes) "<name>"`: the media type must be one the store allows.
fn parse_file_meta(tail: &str) -> Option<(String, u64, String)> {
    let (media, rest) = tail.split_once(", ")?;
    if !MEDIA_TYPES.contains(&media) {
        return None;
    }
    let (size, quoted) = rest.split_once(" bytes) \"")?;
    let name = unescape_name(quoted.strip_suffix('"')?)?;
    Some((media.to_owned(), size.parse().ok()?, name))
}

/// The name between the quotes: `\\` and `\"` stand for themselves, any other backslash or a
/// bare quote is not a name this crate wrote.
fn unescape_name(text: &str) -> Option<String> {
    let mut name = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => name.push(chars.next().filter(|n| matches!(n, '\\' | '"'))?),
            '"' => return None,
            other => name.push(other),
        }
    }
    Some(name)
}

fn parse_range(token: &str) -> Option<(u32, u32)> {
    let (from, to) = token.split_once('-')?;
    Some((from.parse().ok()?, to.parse().ok()?))
}
