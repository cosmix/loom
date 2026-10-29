//! Transcript token accounting and the append-only summary cost ledger.

use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::planning::runtime::RuntimePaths;
use crate::tokens::accumulate::AgentState;
use crate::tokens::AgentKind;
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Estimates and rolling totals derived from recorded summary costs.
pub mod estimate;

// Keep the transcript line bound aligned with tokens::tail::MAX_LINE_BYTES.
const MAX_LINE_BYTES: usize = 32 * 1024 * 1024;
const MAX_LOG_BYTES: usize = 8 * 1024 * 1024;

/// Token counts with absent fields representing unknown usage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenVector {
    /// Input tokens that did not come from the cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fresh_input: Option<u64>,
    /// Tokens written to the prompt cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    /// Tokens read from the prompt cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    /// Generated output tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
}

/// What requested a summary rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Rebuilt during an authoring turn.
    Refinement,
    /// Rebuilt automatically after invalidation.
    Background,
    /// Rebuilt at the user's explicit request.
    Button,
}

/// The scope of the rebuilt summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// A summary of one source document.
    Document,
    /// A summary across a scope's documents.
    Overview,
}

/// One attributed rebuild cost, stored as a single JSON log record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostLine {
    /// Time of attribution in RFC 3339 format.
    pub time: String,
    /// What requested this rebuild.
    pub trigger: Trigger,
    /// Summary ids sharing this cost.
    pub summaries: Vec<String>,
    /// Scope of these summaries.
    pub level: Level,
    /// Model reported by the transcript, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Source size recorded for per-byte estimates, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_bytes: Option<u64>,
    /// Usage counted by the token tracker's accumulator.
    pub tokens: TokenVector,
}

/// Count complete transcript lines; an unreadable transcript has unknown usage.
pub fn transcript_tokens(path: &Path) -> Result<TokenVector> {
    Ok(File::open(path)
        .and_then(|file| read_transcript(file, path))
        .unwrap_or_default())
}

fn read_transcript(file: File, path: &Path) -> io::Result<TokenVector> {
    let mut state = AgentState::new(AgentKind::Main, &path.to_string_lossy());
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        let count = reader
            .by_ref()
            .take(MAX_LINE_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if line.len() > MAX_LINE_BYTES {
            if !line.ends_with(b"\n") {
                reader.skip_until(b'\n')?;
            }
            continue;
        }
        if line.pop() != Some(b'\n') {
            break;
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if let Ok(text) = std::str::from_utf8(&line) {
            state.ingest(text);
        }
    }
    Ok(TokenVector {
        fresh_input: Some(state.total.input),
        cache_write: Some(state.total.cache_write),
        cache_read: Some(state.total.cache_read),
        output: Some(state.total.output),
    })
}

/// Append one complete JSON record under an exclusive lock on the no-follow log.
pub fn append(paths: &RuntimePaths, line: &CostLine) -> Result<()> {
    let mut record = serde_json::to_vec(line).context("serializing summary cost")?;
    record.push(b'\n');
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&paths.summary_cost)
        .context("opening summary cost log")?;
    FileExt::lock_exclusive(&file).context("locking summary cost log")?;
    file.write_all(&record)
        .context("appending summary cost record")?;
    FileExt::unlock(&file).context("unlocking summary cost log")?;
    Ok(())
}

/// Read up to 8 MiB of the ledger, skipping malformed and unfinished records.
/// An absent ledger is empty; unsafe paths and other read errors are reported.
pub fn read_log(paths: &RuntimePaths) -> Result<Vec<CostLine>> {
    let relative = paths
        .summary_cost
        .strip_prefix(&paths.dir)
        .context("summary cost log is outside the runtime directory")?;
    let bytes = match read_bounded(&paths.dir, relative, MAX_LOG_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(bytes
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| line.ends_with(b"\n"))
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect())
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod estimate_tests;
#[cfg(test)]
mod tests;
