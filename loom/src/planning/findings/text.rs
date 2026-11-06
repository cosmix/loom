//! Sanitizers for agent-written ledger text.

use super::body::META_KEYS;
use crate::fs::knowledge::splice::strip_closing_sequence;

/// The longest title, in characters.
const TITLE_CHARS: usize = 80;
/// The longest dismissal reason, in characters.
const REASON_CHARS: usize = 200;
/// The title of a finding whose text leaves nothing to title it with.
const UNTITLED: &str = "Untitled finding";
/// Characters that open a heading or a code fence when they start a line.
const BLOCK_OPENERS: [char; 3] = ['#', '`', '~'];

/// `text` cut to at most `max` characters, on a char boundary.
fn cut(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// `text` on one line: whitespace controls become spaces, other controls are dropped.
pub(super) fn one_line(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// A heading title: the text's first 80 characters on one line. A trailing CommonMark closing
/// sequence is dropped (`Fix ###` becomes `Fix`, `C#` stays) so the heading reads back the same
/// title, using the helper the section reader applies.
pub fn title(text: &str) -> String {
    let flat = one_line(text);
    let mut head = cut(&flat, TITLE_CHARS).trim_end();
    // The section reader strips a closing sequence from the written heading too, so strip until
    // nothing changes: the title then reads back unchanged.
    loop {
        let stripped = strip_closing_sequence(head).trim_end();
        if stripped.len() == head.len() {
            break;
        }
        head = stripped;
    }
    if head.is_empty() {
        UNTITLED.to_owned()
    } else {
        head.to_owned()
    }
}

/// Whether `text` starts with something that needs the escape: a character that opens a heading
/// (`#`) or a code fence (`` ` `` or `~`), a metadata key (the entry reader takes it as metadata
/// when it opens the text), or a backslash, so that [`unescape`] strips only the escape itself.
fn opens_structure(text: &str) -> bool {
    text.starts_with(BLOCK_OPENERS)
        || text.starts_with('\\')
        || META_KEYS.iter().any(|key| text.starts_with(key))
}

/// The text paragraph: the whole text on one line, with a leading backslash when it starts with
/// something `opens_structure` names, so a finding never changes the ledger's structure.
pub fn paragraph(text: &str) -> String {
    let flat = one_line(text);
    if opens_structure(&flat) {
        format!("\\{flat}")
    } else {
        flat
    }
}

/// Undo the escape [`paragraph`] adds.
pub(super) fn unescape(text: &str) -> String {
    let text = text.trim();
    // Exactly one backslash is the escape; a second one belongs to the text.
    match text.strip_prefix('\\') {
        Some(rest) if opens_structure(rest) => rest.to_owned(),
        _ => text.to_owned(),
    }
}

/// A dismissal reason: one line (line breaks and tabs become spaces, other controls are dropped), trimmed, at most 200 characters. `None` when
/// nothing is left.
pub fn reason(text: &str) -> Option<String> {
    let flat = one_line(text);
    let reason = cut(&flat, REASON_CHARS).trim_end();
    (!reason.is_empty()).then(|| reason.to_owned())
}
