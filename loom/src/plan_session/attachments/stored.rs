//! The upload store as a session reads it: `<runtime>/attachments/<id>.<ext>` and its sidecar
//! `<id>.json`. The planning server writes it (`plan_server::uploads`); this module holds the
//! parts both sides agree on, so a path is only ever derived from an id of the right shape and
//! an allow-listed media type, never from a name.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::context::format_chars::is_format;
use crate::fs::safe_read::read_bounded;

/// Hex characters of the content hash that name a file.
pub const ID_LEN: usize = 16;

/// Longest display name kept, in `char`s.
pub const NAME_CHARS: usize = 120;

/// The name shown when a display name cleans to nothing.
pub const DEFAULT_NAME: &str = "attachment";

/// Whether `c` may not appear in a display name: a control character (Cc), or any whitespace but
/// a plain space. U+2028, U+2029, NEL and a tab read as a line break to the model and could fake
/// a chip line in the delivered attachments block.
pub fn is_unsafe_name_char(c: char) -> bool {
    c.is_control() || (c.is_whitespace() && c != ' ')
}

/// `raw` as a display name: without control, format or non-space whitespace characters, trimmed,
/// at most [`NAME_CHARS`] characters, and [`DEFAULT_NAME`] when nothing (or only `.` or `..`) is
/// left. Display text only; it never reaches a path.
pub fn clean_name(raw: &str) -> String {
    let kept: String = raw
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

/// The media types an upload may have.
pub const MEDIA_TYPES: [&str; 7] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "application/pdf",
    "text/plain",
    "text/markdown",
];

/// Largest sidecar read back; a real one is a few hundred bytes.
const SIDECAR_MAX: usize = 4096;

/// What the store keeps beside a file. Never a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    pub name: String,
    pub media_type: String,
    pub bytes: u64,
    /// Seconds since the Unix epoch of the last upload or draft write that named the file.
    pub stored_at: u64,
}

/// The file extension of an allow-listed media type.
pub fn extension(media_type: &str) -> Option<&'static str> {
    match media_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "application/pdf" => Some("pdf"),
        "text/plain" => Some("txt"),
        "text/markdown" => Some("md"),
        _ => None,
    }
}

/// Whether `id` is [`ID_LEN`] lowercase hex characters, the only shape a path is built from.
pub fn valid_id(id: &str) -> bool {
    id.len() == ID_LEN && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `<dir>/<id>.<ext>` for a valid `id` and an allow-listed `media_type`.
pub fn data_path(dir: &Path, id: &str, media_type: &str) -> Option<PathBuf> {
    let ext = extension(media_type)?;
    valid_id(id).then(|| dir.join(format!("{id}.{ext}")))
}

/// The sidecar file name of `id`.
pub fn sidecar_name(id: &str) -> String {
    format!("{id}.json")
}

/// The sidecar of `id`, or `None` for an id of the wrong shape, a missing or unreadable file,
/// or a sidecar naming a media type outside the allow list.
pub fn read_sidecar(dir: &Path, id: &str) -> Option<Sidecar> {
    if !valid_id(id) {
        return None;
    }
    let bytes = read_bounded(dir, Path::new(&sidecar_name(id)), SIDECAR_MAX).ok()?;
    let sidecar: Sidecar = serde_json::from_slice(&bytes).ok()?;
    extension(&sidecar.media_type)?;
    Some(sidecar)
}

/// The stored file of `id` and its sidecar, when both exist and the file is a regular file (a
/// symlink in its place is refused).
pub fn locate(dir: &Path, id: &str) -> Option<(PathBuf, Sidecar)> {
    let sidecar = read_sidecar(dir, id)?;
    let path = data_path(dir, id, &sidecar.media_type)?;
    let meta = std::fs::symlink_metadata(&path).ok()?;
    meta.is_file().then_some((path, sidecar))
}
