//! The `<loom-attachments>` block of a delivered message: format, parse and resolve.

mod format;
mod parse;
mod pasted;
mod resolve;
pub mod stored;
#[cfg(test)]
mod tests;

pub use format::format_message;
pub use parse::{parse_message, parse_unwrapped};
pub use pasted::unwrap_pasted;
pub use resolve::{resolve, resolve_file, resolve_input, without_file_paths};

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_ATTACHMENTS: usize = 10;
pub const MAX_QUOTE_BYTES: usize = 16 * 1024;
pub const QUOTE_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Source,
    Summary,
    /// An uploaded file, named by its stored id rather than a document path.
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockedTag {
    Running,
    Done,
    Retired,
    Knowledge,
    Brief,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub path: String,
    pub anchor: Option<String>,
    pub lines: Option<(u32, u32)>,
    pub stage: Option<String>,
    pub summary_sentence: Option<String>,
    pub rests_on: Vec<String>,
    pub quote: String,
    pub changed_since_selected: bool,
    pub locked: Option<LockedTag>,
    /// What an uploaded file chip shows. Set exactly when `kind` is `File`, whose `path` is the
    /// stored file's absolute path while the block is formatted and empty in every chip a
    /// browser receives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<FileMeta>,
}

/// An uploaded file as the browser sees it: the id `POST /api/attachments` answered, the display
/// name, the checked media type and the size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub bytes: u64,
}

/// An attachment as a browser sends it: a stored file by id, or a document selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttachmentInput {
    File(FileAttachmentInput),
    Document(DocumentAttachmentInput),
}

/// The only tag a file input carries, so a document input cannot pass as a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileTag {
    File,
}

/// `{"kind": "file", "id": ..., "name": ...}`. The client never sends a path; `name` is only
/// echoed back and the stored name wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileAttachmentInput {
    pub kind: FileTag,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DocumentAttachmentInput {
    pub kind: AttachmentKind,
    pub path: String,
    pub anchor: Option<String>,
    pub lines: Option<(u32, u32)>,
    pub stage: Option<String>,
    pub summary_sentence: Option<String>,
    pub rests_on: Vec<String>,
    pub quote: String,
    pub selected_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentError {
    Path(String),
    NotADocument,
    /// The id is not a stored file's: wrong shape, or nothing stored under it.
    UnknownFile(String),
    QuoteTooLarge {
        bytes: usize,
    },
    RestsOn(String),
    SentenceId,
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => write!(f, "invalid attachment path: {path}"),
            Self::NotADocument => write!(f, "attachment is not a document"),
            Self::UnknownFile(id) => write!(f, "attachment file is not stored: {id}"),
            Self::QuoteTooLarge { bytes } => write!(
                f,
                "attachment quote is too large: {bytes} bytes (maximum {MAX_QUOTE_BYTES})"
            ),
            Self::RestsOn(address) => write!(f, "invalid rests-on address: {address}"),
            Self::SentenceId => write!(f, "invalid summary sentence id"),
        }
    }
}

impl std::error::Error for AttachmentError {}
