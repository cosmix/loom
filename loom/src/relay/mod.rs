//! Relay protocol contract shared by the CLI ticket writer, the relay hook
//! helper, the daemon's inbox drain and the capsule builder.
//!
//! Carries types, parsing and pure rules only (plus the scratch-directory
//! helpers, which touch the filesystem to create and validate one 0700
//! directory). No wiring into any command lives here — see
//! `doc/plans/PLAN-loom-state-confinement.md` sections 4-8.

pub mod emit;
mod inbox;
mod kind;
mod line;
mod matrix;
mod payload;
mod scratch;
mod ticket;

pub use inbox::{AgentRole, InboxEntry};
pub use kind::RequestKind;
pub use line::RelayLine;
pub use matrix::{verdict, MatrixVerdict};
pub use payload::{decode_payload, HandoffRequest, RequestPayload, VerdictRequest};
pub use scratch::{
    ensure_dir_0700, scratch_root, scratch_root_from_env, session_dir, validate_session_dir,
    Platform,
};
pub use ticket::{sha256_hex, Ticket};

/// Bound on one relay line: `LOOM_RELAY_V1 kind=... id=... sha256=... bytes=...`.
pub const MAX_LINE_BYTES: usize = 160;
/// At most this many relay lines are extracted from one tool call's output.
pub const MAX_LINES_PER_CALL: usize = 16;
/// A session may accumulate at most this many CLI tickets the relay hook has
/// not yet picked up before the CLI refuses to write another.
pub const MAX_UNCONSUMED_TICKETS: usize = 32;
/// The daemon refuses to relay past this many pending (undrained) inbox
/// entries for one session.
pub const MAX_PENDING_ENTRIES: usize = 128;
/// A persisted-output file the relay hook reads in place of inline stdout is
/// capped at this size.
pub const MAX_PERSISTED_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
/// A ticket may not exceed the daemon's own request-frame limit — the socket
/// and relay paths share one cap so neither becomes the softer one.
pub const MAX_TICKET_BYTES: usize = crate::daemon::MAX_REQUEST_BYTES;

/// 128 random bits as 32 lowercase hex characters — the same shape the
/// completion nonce uses (`commands/stage/control_complete.rs`).
pub fn new_request_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_request_id_is_thirty_two_lowercase_hex_characters() {
        let id = new_request_id();
        assert_eq!(id.len(), 32);
        assert!(id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn new_request_id_is_unique_across_calls() {
        assert_ne!(new_request_id(), new_request_id());
    }
}
