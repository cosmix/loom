//! The `<loom-attachments>` block of a delivered message: format, parse and resolve.

mod format;
mod parse;
mod pasted;
mod resolve;
#[cfg(test)]
mod tests;

pub use format::format_message;
pub use parse::{parse_message, parse_unwrapped};
pub use pasted::unwrap_pasted;
pub use resolve::resolve;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AttachmentInput {
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
    QuoteTooLarge { bytes: usize },
    RestsOn(String),
    SentenceId,
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => write!(f, "invalid attachment path: {path}"),
            Self::NotADocument => write!(f, "attachment is not a document"),
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
