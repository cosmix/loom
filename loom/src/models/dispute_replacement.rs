//! The replacement file a frozen-contract dispute can carry.

/// Most bytes of a replacement for a frozen contract file. It travels inline
/// in the dispute request, whose relay ticket and RPC frame are capped at
/// 64 KiB (`relay::MAX_TICKET_BYTES`). JSON escaping expands a control
/// character to six bytes, so this bound alone does not guarantee a fitting
/// ticket: the CLI also measures the encoded ticket
/// (`commands/stage/dispute_kinds.rs`).
pub const MAX_REPLACEMENT_BYTES: usize = 32 * 1024;
