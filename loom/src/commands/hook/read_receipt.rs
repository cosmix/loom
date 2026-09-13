//! Hidden `loom hook read-receipt` adapter.
//!
//! Receipt logic lives in `context::read_receipts` so it can be proven before
//! this hidden command is registered by the CLI worker.

use anyhow::Result;
use std::io::{Read, Write};

use crate::context::read_receipts::{self, Mode};

const MAX_STDIN_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadReceiptMode {
    Prepare,
    Check,
    Complete,
}

/// Read the raw hook payload from stdin and emit only a proven check result.
///
/// Hooks must not block tool execution for malformed input or bookkeeping
/// failures, so every error is deliberately represented by an empty result.
pub fn read_receipt(mode: ReadReceiptMode) -> Result<()> {
    let Some(raw) = read_stdin() else {
        return Ok(());
    };
    let proven = read_receipts::invoke(map_mode(mode), &raw);
    if mode == ReadReceiptMode::Check {
        if let Some(count) = proven {
            let output = proven_output(count);
            let _ = std::io::stdout().lock().write_all(output.as_bytes());
        }
    }
    Ok(())
}

fn read_stdin() -> Option<String> {
    let mut raw = String::new();
    std::io::stdin()
        .lock()
        .take(MAX_STDIN_BYTES + 1)
        .read_to_string(&mut raw)
        .ok()?;
    (raw.len() <= MAX_STDIN_BYTES as usize).then_some(raw)
}

fn map_mode(mode: ReadReceiptMode) -> Mode {
    match mode {
        ReadReceiptMode::Prepare => Mode::Prepare,
        ReadReceiptMode::Check => Mode::Check,
        ReadReceiptMode::Complete => Mode::Complete,
    }
}

fn proven_output(count: usize) -> String {
    format!("proven {count}\n")
}

#[cfg(test)]
mod tests {
    use super::proven_output;

    #[test]
    fn check_output_includes_the_distinct_proven_delivery_count() {
        assert_eq!(proven_output(2), "proven 2\n");
    }
}
