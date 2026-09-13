//! The JSON ticket a CLI invocation writes to its scratch directory before
//! emitting the matching `LOOM_RELAY_V1` line.

use super::kind::RequestKind;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Wire version this module reads and writes. [`Ticket::decode`] refuses
/// anything else.
const TICKET_VERSION: u32 = 1;

/// `$LOOM_SCRATCH_DIR/<id>.req`, written via `.<id>.tmp` plus rename.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub v: u32,
    pub id: String,
    pub kind: RequestKind,
    pub created_at: DateTime<Utc>,
    pub payload: Value,
}

impl Ticket {
    /// Serialize to the bytes written to disk. Infallible: every field type
    /// here (strings, an enum, a timestamp, and a JSON `Value`, which cannot
    /// hold a non-finite float) always serializes.
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Ticket always serializes to JSON")
    }

    /// Parse and require `v == 1`.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let ticket: Self = serde_json::from_slice(bytes).context("ticket did not parse as JSON")?;
        if ticket.v != TICKET_VERSION {
            bail!(
                "unsupported ticket version {}, expected {TICKET_VERSION}",
                ticket.v
            );
        }
        Ok(ticket)
    }
}

/// Lowercase hex SHA-256, matching the `sha256=` field of a relay line.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn sample() -> Ticket {
        Ticket {
            v: 1,
            id: "4f1c9e0a7b2d4c6e8f00112233445566".to_string(),
            kind: RequestKind::Memory,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
            payload: serde_json::json!({"content": "note"}),
        }
    }

    #[test]
    fn round_trips_through_encode_and_decode() {
        let ticket = sample();
        assert_eq!(Ticket::decode(&ticket.encode()).unwrap(), ticket);
    }

    #[test]
    fn decode_refuses_an_unsupported_version() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["v"] = serde_json::json!(2);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(Ticket::decode(&bytes)
            .unwrap_err()
            .to_string()
            .contains("unsupported ticket version"));
    }

    #[test]
    fn decode_refuses_an_unknown_field() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["extra"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(Ticket::decode(&bytes).is_err());
    }

    #[test]
    fn decode_refuses_malformed_json() {
        assert!(Ticket::decode(b"not json").is_err());
    }

    #[test]
    fn sha256_hex_matches_the_known_empty_digest() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
