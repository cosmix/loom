//! Identifiers and limits for native workers.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};

/// Maximum simultaneously active workers in a stage.
pub const MAX_ACTIVE_PER_STAGE: usize = 6;
/// Default worker deadline in seconds.
pub const NATIVE_DEADLINE_SECS: u64 = 1500;
/// Maximum brief size in bytes.
pub const MAX_BRIEF_BYTES: u64 = 262_144;
/// Maximum protocol frame size in bytes.
pub const MAX_FRAME_BYTES: usize = 1_048_576;
/// Maximum record size in bytes.
pub const MAX_RECORD_BYTES: u64 = 4_194_304;
/// Bytes of `events.jsonl` only settlement records (terminal and ownership) may use:
/// room for three records of `SETTLEMENT_TEXT_BYTES` of text each, with their envelopes.
pub const SETTLEMENT_RESERVE_BYTES: u64 = 16_384;
/// Longest `reason`, `by` or `detail` a settlement record keeps, in bytes of its
/// JSON-escaped form.
pub const SETTLEMENT_TEXT_BYTES: usize = 2_048;
/// Default report size in bytes.
pub const REPORT_DEFAULT_BYTES: usize = 8_192;
/// Maximum number of queued messages.
pub const MAX_QUEUED_MESSAGES: usize = 16;
/// Maximum message size in bytes.
pub const MAX_MESSAGE_BYTES: usize = 16_384;
/// Grace period after interruption in seconds.
pub const INTERRUPT_GRACE_SECS: u64 = 5;
/// Grace period after EOF in seconds.
pub const EOF_GRACE_SECS: u64 = 2;
/// Grace period for reaping in seconds.
pub const REAP_GRACE_SECS: u64 = 2;
/// Minimum progress event interval in seconds.
pub const PROGRESS_EVENT_INTERVAL_SECS: u64 = 30;

#[derive(Debug, PartialEq, Eq)]
pub enum IdError {
    Empty,
    TooLong,
    BadCharacter(char),
    BadShape,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "identifier is empty"),
            Self::TooLong => write!(f, "identifier is too long"),
            Self::BadCharacter(c) => write!(f, "invalid identifier character: {c:?}"),
            Self::BadShape => write!(f, "invalid identifier shape"),
        }
    }
}

impl std::error::Error for IdError {}

/// Validate an identifier matching `^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$`.
pub fn validate_unit_id(value: &str) -> Result<(), IdError> {
    if value.is_empty() {
        return Err(IdError::Empty);
    }
    if value.len() > 64 {
        return Err(IdError::TooLong);
    }
    if value.starts_with(['-', '_']) {
        return Err(IdError::BadShape);
    }
    for c in value.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
            return Err(IdError::BadCharacter(c));
        }
    }
    Ok(())
}

/// Validate a stage identifier with loom's stage-id rule (`crate::validation::validate_id`):
/// at most 128 ASCII alphanumerics, `-` and `_`, a leading `-` or `_` allowed. The rule
/// excludes `/` and `.`, so a valid stage id is one safe path component.
pub fn validate_stage_id(value: &str) -> Result<(), IdError> {
    if value.is_empty() {
        return Err(IdError::Empty);
    }
    if value.len() > crate::validation::MAX_ID_LENGTH {
        return Err(IdError::TooLong);
    }
    if let Some(c) = value
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && *c != '-' && *c != '_')
    {
        return Err(IdError::BadCharacter(c));
    }
    crate::validation::validate_id(value).map_err(|_| IdError::BadShape)
}

/// Validate a message identifier using the unit identifier rules.
pub fn validate_message_id(value: &str) -> Result<(), IdError> {
    validate_unit_id(value)
}

/// Validate `inv-` followed by exactly 32 lowercase hexadecimal digits.
pub fn validate_invocation_id(value: &str) -> Result<(), IdError> {
    if value.is_empty() {
        return Err(IdError::Empty);
    }
    if value.len() > 36 {
        return Err(IdError::TooLong);
    }
    if value.len() != 36 || !value.starts_with("inv-") {
        return Err(IdError::BadShape);
    }
    for c in value[4..].chars() {
        if !c.is_ascii_digit() && !('a'..='f').contains(&c) {
            return Err(IdError::BadCharacter(c));
        }
    }
    Ok(())
}

/// Mint an invocation identifier from 16 bytes of operating system randomness.
pub fn mint_invocation_id() -> io::Result<String> {
    let mut bytes = [0_u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(format!("inv-{}", hex::encode(bytes)))
}

#[cfg(test)]
#[path = "ids_tests.rs"]
mod tests;
