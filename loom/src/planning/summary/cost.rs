//! Transcript token accounting and the append-only summary cost ledger.

use crate::fs::safe_read::open_regular_no_follow;
use crate::planning::runtime::RuntimePaths;
use crate::tokens::accumulate::AgentState;
use crate::tokens::tail::{self, Line};
use crate::tokens::AgentKind;
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Estimates and rolling totals derived from recorded summary costs.
pub mod estimate;

/// Largest tail of the ledger read.
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

/// Count complete transcript lines; an unreadable transcript has unknown usage. The read is
/// the token tracker's: no-follow, non-blocking, complete lines only, bounded line length.
pub fn transcript_tokens(path: &Path) -> Result<TokenVector> {
    let Some(stat) = tail::stat(path) else {
        return Ok(TokenVector::default());
    };
    let mut state = AgentState::new(AgentKind::Main, &path.to_string_lossy());
    let stop = AtomicBool::new(false);
    let read = tail::read_lines(path, &mut 0, stat.len, &stop, |line| {
        if let Line::Text(text) = line {
            state.ingest(text);
        }
    });
    if read.is_err() {
        return Ok(TokenVector::default());
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

/// Read the last 8 MiB of the ledger, skipping malformed and unfinished records; older records
/// fall out of estimates and totals. An absent ledger is empty; unsafe paths, other read errors
/// and a tail holding no complete record are reported.
pub fn read_log(paths: &RuntimePaths) -> Result<Vec<CostLine>> {
    let relative = paths
        .summary_cost
        .strip_prefix(&paths.dir)
        .context("summary cost log is outside the runtime directory")?;
    let relative = relative
        .to_str()
        .context("summary cost log path is not UTF-8")?;
    let Some(file) = open_regular_no_follow(&paths.dir, relative, libc::O_RDONLY)? else {
        return Ok(Vec::new());
    };
    let bytes = read_tail(file, MAX_LOG_BYTES as u64)?;
    Ok(bytes
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| line.ends_with(b"\n"))
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect())
}

/// The last `cap` bytes of `file`. A cut inside a record drops that partial first line, so the
/// bytes start at a record; a tail of one unbroken line is an error, not an empty ledger.
fn read_tail(mut file: File, cap: u64) -> Result<Vec<u8>> {
    let len = file
        .metadata()
        .context("inspecting summary cost log")?
        .len();
    let start = len.saturating_sub(cap);
    file.seek(SeekFrom::Start(start))
        .context("seeking in summary cost log")?;
    let mut bytes = Vec::new();
    file.take(cap)
        .read_to_end(&mut bytes)
        .context("reading summary cost log")?;
    if start > 0 {
        let first = bytes.iter().position(|byte| *byte == b'\n');
        let first = first
            .with_context(|| format!("no complete record in the last {cap} bytes of the log"))?;
        bytes.drain(..=first);
    }
    Ok(bytes)
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod estimate_tests;
#[cfg(test)]
mod feature_tests;
#[cfg(test)]
mod tests;
