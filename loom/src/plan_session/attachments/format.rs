//! Writes the `<loom-attachments>` block after a message text.
//!
//! Also holds the block's shared syntax (tags, names, escaping) that `parse` reads back.

use super::{Attachment, AttachmentKind, LockedTag, QUOTE_CHARS};

pub(super) const OPEN_TAG: &str = "<loom-attachments>";
pub(super) const CLOSE_TAG: &str = "</loom-attachments>";
const CUT_HEAD: &str = "[…";
const CUT_TAIL: &str = " more characters; read the source]";

/// The delivered text: the user's text, then (when there are attachments) a blank line and the
/// `<loom-attachments>` block. A text line that looks like a block tag is escaped even when no
/// block follows, so `parse_unwrapped` reads every text back unchanged and a typed fake block is
/// never read as chips. This output was never wrapped by Claude Code: read it back with
/// `parse_unwrapped`, since `parse_message` would unwrap a typed `<pasted_content>` wrapper.
pub fn format_message(text: &str, attachments: &[Attachment]) -> String {
    let mut out = escape_text(text);
    if attachments.is_empty() {
        return out;
    }
    out.push_str("\n\n");
    out.push_str(OPEN_TAG);
    out.push('\n');
    for (index, attachment) in attachments.iter().enumerate() {
        out.push_str(&header(index + 1, attachment));
        out.push('\n');
        for line in quote_lines(&attachment.quote) {
            out.push_str("> ");
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push_str(CLOSE_TAG);
    out
}

pub(super) fn kind_name(kind: &AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::Source => "source",
        AttachmentKind::Summary => "summary",
    }
}

pub(super) fn kind_from(name: &str) -> Option<AttachmentKind> {
    match name {
        "source" => Some(AttachmentKind::Source),
        "summary" => Some(AttachmentKind::Summary),
        _ => None,
    }
}

fn locked_name(tag: &LockedTag) -> &'static str {
    match tag {
        LockedTag::Running => "running",
        LockedTag::Done => "done",
        LockedTag::Retired => "retired",
        LockedTag::Knowledge => "knowledge",
        LockedTag::Brief => "brief",
    }
}

pub(super) fn locked_from(name: &str) -> Option<LockedTag> {
    match name {
        "running" => Some(LockedTag::Running),
        "done" => Some(LockedTag::Done),
        "retired" => Some(LockedTag::Retired),
        "knowledge" => Some(LockedTag::Knowledge),
        "brief" => Some(LockedTag::Brief),
        _ => None,
    }
}

/// For a line matching `^\\*</?loom-attachments>$` after trimming: the byte offset of its first
/// character that is neither whitespace nor a control, and the number of backslashes before the
/// tag. Control characters (except tab) are ignored when matching, since the bridge's `sanitize`
/// removes them before delivery: `<loom-attach\x01ments>` becomes a real tag line there, so it
/// must be escaped here.
pub(super) fn tag_line(line: &str) -> Option<(usize, usize)> {
    let visible: String = line
        .chars()
        .filter(|&c| !c.is_control() || c == '\t')
        .collect();
    let trimmed = visible.trim();
    let bare = trimmed.trim_start_matches('\\');
    if bare != OPEN_TAG && bare != CLOSE_TAG {
        return None;
    }
    let offset = line
        .char_indices()
        .find(|(_, c)| !c.is_whitespace() && !c.is_control())
        .map(|(index, _)| index)?;
    Some((offset, trimmed.len() - bare.len()))
}

fn escape_text(text: &str) -> String {
    let lines: Vec<String> = text
        .split('\n')
        .map(|line| match tag_line(line) {
            Some((offset, _)) => format!("{}\\{}", &line[..offset], &line[offset..]),
            None => line.to_owned(),
        })
        .collect();
    lines.join("\n")
}

fn header(number: usize, attachment: &Attachment) -> String {
    let kind = kind_name(&attachment.kind);
    let mut header = format!("[{number}] {kind} {}", address(attachment));
    if let Some((from, to)) = attachment.lines {
        header.push_str(&format!(" lines {from}-{to}"));
    }
    if !attachment.rests_on.is_empty() {
        header.push_str(&format!(" rests-on {}", attachment.rests_on.join(",")));
    }
    if let Some(tag) = &attachment.locked {
        header.push_str(&format!(" locked={}", locked_name(tag)));
    }
    if attachment.changed_since_selected {
        header.push_str(" changed-since-selected");
    }
    header
}

/// `path#anchor`: `path#stage:<id>` for a stage, the sentence id for a summary without an anchor.
fn address(attachment: &Attachment) -> String {
    let anchor = match (&attachment.stage, &attachment.anchor) {
        (Some(stage), _) => Some(format!("stage:{stage}")),
        (None, Some(anchor)) => Some(anchor.clone()),
        (None, None) => attachment.summary_sentence.clone(),
    };
    match anchor {
        Some(anchor) => format!("{}#{anchor}", attachment.path),
        None => attachment.path.clone(),
    }
}

/// Whether `line` is the marker `[…N more characters; read the source]`.
fn is_cut_marker(line: &str) -> bool {
    let count = line
        .strip_prefix(CUT_HEAD)
        .and_then(|rest| rest.strip_suffix(CUT_TAIL));
    count.is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The quote's lines. A quote over `QUOTE_CHARS` characters keeps the lines that fit in that
/// many characters (newlines counted), then the marker. A quote already ending in a marker is
/// left alone, so a parsed message formats back to itself.
fn quote_lines(quote: &str) -> Vec<String> {
    let lines: Vec<&str> = quote.split('\n').collect();
    let total = quote.chars().count();
    if total <= QUOTE_CHARS || lines.last().is_some_and(|line| is_cut_marker(line)) {
        return lines.into_iter().map(str::to_owned).collect();
    }
    let (mut kept, mut shown) = (0, 0);
    for line in &lines {
        let width = line.chars().count() + usize::from(kept > 0);
        if shown + width > QUOTE_CHARS {
            break;
        }
        shown += width;
        kept += 1;
    }
    let mut out: Vec<String> = lines[..kept]
        .iter()
        .map(|line| (*line).to_owned())
        .collect();
    if kept == 0 {
        // The first line alone is over the limit: cut it on a character boundary.
        out.push(lines[0].chars().take(QUOTE_CHARS).collect());
        shown = QUOTE_CHARS;
    }
    out.push(format!("{CUT_HEAD}{}{CUT_TAIL}", total - shown));
    out
}
