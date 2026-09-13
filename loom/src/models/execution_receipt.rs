use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const EXECUTION_RECEIPT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub schema_version: u16,
    pub provider: ReceiptProvider,
    pub observed_at: DateTime<Utc>,
    pub request_id: Option<String>,
    pub usage: ReceiptUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptProvider {
    Claude,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub thinking_output_tokens: Option<u64>,
    pub cache_write_5m_input_tokens: Option<u64>,
    pub cache_write_1h_input_tokens: Option<u64>,
}

#[derive(Debug)]
pub enum ReceiptDecodeError {
    UnsupportedVersion(u64),
    Invalid(serde_json::Error),
    InvalidSchemaVersion,
    ThinkingExceedsOutput,
}

impl ReceiptDecodeError {
    pub fn is_unsupported_version(&self) -> bool {
        matches!(self, Self::UnsupportedVersion(_))
    }
}

impl fmt::Display for ReceiptDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported execution receipt schema version {version}"
                )
            }
            Self::Invalid(error) => write!(formatter, "invalid execution receipt: {error}"),
            Self::InvalidSchemaVersion => {
                write!(formatter, "invalid execution receipt schema_version")
            }
            Self::ThinkingExceedsOutput => {
                write!(
                    formatter,
                    "execution receipt thinking tokens exceed output tokens"
                )
            }
        }
    }
}

impl std::error::Error for ReceiptDecodeError {}

pub fn decode(input: &str) -> Result<ExecutionReceipt, ReceiptDecodeError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(ReceiptDecodeError::Invalid)?;
    let version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(ReceiptDecodeError::InvalidSchemaVersion)?;
    if version != u64::from(EXECUTION_RECEIPT_SCHEMA_VERSION) {
        return Err(ReceiptDecodeError::UnsupportedVersion(version));
    }
    let receipt: ExecutionReceipt =
        serde_json::from_value(value).map_err(ReceiptDecodeError::Invalid)?;
    if receipt
        .usage
        .thinking_output_tokens
        .is_some_and(|thinking| thinking > receipt.usage.output_tokens)
    {
        return Err(ReceiptDecodeError::ThinkingExceedsOutput);
    }
    Ok(receipt)
}

#[cfg(test)]
#[path = "execution_receipt_tests.rs"]
mod tests;
