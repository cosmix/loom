//! The replacement files a frozen-contract dispute can carry, and the
//! per-contract rulings its verdict returns.

use serde::{Deserialize, Serialize};

use super::Citation;

/// Most bytes of a replacement for a frozen contract file. It travels inline
/// in the dispute request, whose relay ticket and RPC frame are capped at
/// 64 KiB (`relay::MAX_TICKET_BYTES`). JSON escaping expands a control
/// character to six bytes, so this bound alone does not guarantee a fitting
/// ticket: the CLI also measures the encoded ticket
/// (`commands/stage/dispute_kinds.rs`).
pub const MAX_REPLACEMENT_BYTES: usize = 32 * 1024;

/// The complete file a contract dispute proposes in place of one frozen file.
/// `file` is the path form the freeze record's `files[].path` uses.
/// `base_sha256` is the frozen file's sha256 the replacement was filed
/// against: only admission sets it, and it overwrites any value a client sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReplacement {
    pub file: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha256: Option<String>,
}

/// The adjudicator's ruling on one disputed contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractRuling {
    pub contract: String,
    pub ruling: ContractRulingKind,
    pub reasoning: String,
    #[serde(default)]
    pub citations: Vec<Citation>,
}

/// Whether a disputed contract's change is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContractRulingKind {
    Accept,
    Reject,
}
