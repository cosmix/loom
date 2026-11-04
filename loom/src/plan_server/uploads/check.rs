//! What the handler checks about an upload before it is stored: the bytes against the declared
//! media type, and the display name.

use crate::plan_server::presence::is_format;
use crate::plan_session::attachments::stored::{is_unsafe_name_char, NAME_CHARS};

/// The name shown when the browser sent none, or one that cleans to nothing.
pub(super) const DEFAULT_NAME: &str = "attachment";

const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Refuse a body whose bytes contradict `media_type`: the signature of an image or a PDF, or
/// UTF-8 without a NUL byte for text. The reason is one line for the 422 answer.
pub(super) fn content(media_type: &str, body: &[u8]) -> Result<(), &'static str> {
    if body.is_empty() {
        return Err("the file is empty");
    }
    let matches = match media_type {
        "image/png" => body.starts_with(PNG),
        "image/jpeg" => body.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/gif" => body.starts_with(b"GIF87a") || body.starts_with(b"GIF89a"),
        "image/webp" => body.len() >= 12 && &body[..4] == b"RIFF" && &body[8..12] == b"WEBP",
        "application/pdf" => body.starts_with(b"%PDF-"),
        "text/plain" | "text/markdown" => return text(body),
        _ => return Err("this media type is not accepted"),
    };
    matches
        .then_some(())
        .ok_or("the file's bytes do not match its media type")
}

fn text(body: &[u8]) -> Result<(), &'static str> {
    if body.contains(&0) {
        return Err("a text file must not contain NUL bytes");
    }
    std::str::from_utf8(body)
        .map(|_| ())
        .map_err(|_| "a text file must be UTF-8")
}

/// The name to show: the final path component of `raw`, without control or format characters
/// (bidirectional overrides included) or whitespace other than a plain space, at most [`NAME_CHARS`] characters, and [`DEFAULT_NAME`]
/// when nothing is left. It is display text only and never reaches a path.
pub(super) fn display_name(raw: Option<&str>) -> String {
    let trimmed = raw.unwrap_or("").trim_end_matches(['/', '\\']);
    let last = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    let kept: String = last
        .chars()
        .filter(|&c| !is_unsafe_name_char(c) && !is_format(c))
        .collect();
    let name: String = kept.trim().chars().take(NAME_CHARS).collect();
    let name = name.trim_end();
    match name {
        "" | "." | ".." => DEFAULT_NAME.to_owned(),
        _ => name.to_owned(),
    }
}
