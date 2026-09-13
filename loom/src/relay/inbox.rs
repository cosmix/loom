//! One relayed request as recorded under `W/inbox/<session-id>/<id>.json`.

use super::kind::RequestKind;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire version this module reads and writes. [`InboxEntry::decode`] refuses
/// anything else.
const INBOX_ENTRY_VERSION: u32 = 1;

/// Which side of the session relayed the request: the lead process, or a
/// teammate inside its process tree. Control kinds never reach this type for
/// a subagent — the relay hook drops them first (`RequestKind::is_control`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    Main,
    Subagent,
}

/// The relay hook's write to the daemon-owned inbox: one file per relayed
/// ticket, attributed to the session and stage the hook's own environment
/// named, not to anything the ticket claims about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxEntry {
    pub v: u32,
    pub id: String,
    pub kind: RequestKind,
    pub relayed_at: DateTime<Utc>,
    pub session_id: String,
    pub stage_id: String,
    pub agent: AgentRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    pub payload: Value,
}

impl InboxEntry {
    /// Serialize for the `<id>.json` write. Infallible for the same reason as
    /// [`super::Ticket::encode`].
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("InboxEntry always serializes to JSON")
    }

    /// Parse and require `v == 1`.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let entry: Self =
            serde_json::from_slice(bytes).context("inbox entry did not parse as JSON")?;
        if entry.v != INBOX_ENTRY_VERSION {
            bail!(
                "unsupported inbox entry version {}, expected {INBOX_ENTRY_VERSION}",
                entry.v
            );
        }
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn sample() -> InboxEntry {
        InboxEntry {
            v: 1,
            id: "4f1c9e0a7b2d4c6e8f00112233445566".to_string(),
            kind: RequestKind::Memory,
            relayed_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
            session_id: "session-1".to_string(),
            stage_id: "stage-a".to_string(),
            agent: AgentRole::Main,
            tool_use_id: Some("tool-1".to_string()),
            payload: serde_json::json!({"content": "note"}),
        }
    }

    #[test]
    fn round_trips_through_encode_and_decode() {
        let entry = sample();
        assert_eq!(InboxEntry::decode(&entry.encode()).unwrap(), entry);
    }

    #[test]
    fn decode_refuses_an_unsupported_version() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["v"] = serde_json::json!(7);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(InboxEntry::decode(&bytes)
            .unwrap_err()
            .to_string()
            .contains("unsupported inbox entry version"));
    }

    #[test]
    fn decode_refuses_an_unknown_field() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["extra"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(InboxEntry::decode(&bytes).is_err());
    }

    #[test]
    fn tool_use_id_is_optional() {
        let mut entry = sample();
        entry.tool_use_id = None;
        assert_eq!(InboxEntry::decode(&entry.encode()).unwrap(), entry);
    }
}
