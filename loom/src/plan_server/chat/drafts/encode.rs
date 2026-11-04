//! The bytes of a draft file, which never exceed what the store reads back.

use anyhow::{bail, Context, Result};

use super::{Draft, MAX_FILE_BYTES};

/// `draft` as pretty JSON of at most [`MAX_FILE_BYTES`]. JSON escaping and indentation can push
/// a draft of full messages past the cap even though each message is within its own; the
/// oldest messages after the first go until it fits, as for the message-count cap. A draft that
/// does not fit with its first message alone is refused: an unreadable file would be skipped on
/// every read and could never be shown or deleted by its owner.
pub(super) fn within_cap(draft: &Draft) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(draft).context("cannot encode the draft")?;
    if bytes.len() <= MAX_FILE_BYTES {
        return Ok(bytes);
    }
    let mut trimmed = draft.clone();
    while bytes.len() > MAX_FILE_BYTES && trimmed.messages.len() > 1 {
        trimmed.messages.remove(1);
        bytes = serde_json::to_vec_pretty(&trimmed).context("cannot encode the draft")?;
    }
    if bytes.len() > MAX_FILE_BYTES {
        bail!(
            "the draft takes {} bytes, over the {MAX_FILE_BYTES} cap",
            bytes.len()
        );
    }
    tracing::warn!(
        session = %draft.session,
        kept = trimmed.messages.len(),
        "kickoff draft over the file cap: older messages dropped"
    );
    Ok(bytes)
}
