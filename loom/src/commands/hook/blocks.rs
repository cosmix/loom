//! Read and summarize the diagnostic ledgers written by shell guards.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use super::target::non_empty_env;
use crate::fs::work_dir::WorkDir;

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LINES: usize = 50_000;
const MAX_LINE_BYTES: usize = 4096;

#[derive(Deserialize)]
pub(crate) struct BlockRecord {
    pub(crate) ts: DateTime<Utc>,
    pub(crate) stage: String,
    #[allow(dead_code)]
    pub(crate) session: String,
    pub(crate) role: String,
    pub(crate) guard: String,
    pub(crate) rule: String,
    #[allow(dead_code)]
    pub(crate) tool: String,
    pub(crate) subject: String,
}

#[derive(Default)]
pub(crate) struct Summary {
    pub(crate) records: Vec<BlockRecord>,
    pub(crate) malformed: usize,
}

pub fn blocks(since_hours: u64, by_rule: bool) -> Result<()> {
    let dir = match non_empty_env("LOOM_WORK_DIR") {
        Some(root) => PathBuf::from(root).join("heartbeat"),
        None => WorkDir::new(".")?.root().join("heartbeat"),
    };
    println!("reading {}", dir.display());
    match fs::metadata(&dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            println!("no ledger directory at {}", dir.display());
            return Ok(());
        }
        result => {
            result.with_context(|| format!("Inspecting ledger directory {}", dir.display()))?;
        }
    }
    println!(
        "{}",
        render(&summarize(&dir, Utc::now(), since_hours)?, by_rule)
    );
    Ok(())
}

pub(crate) fn summarize(
    heartbeat_dir: &Path,
    now: DateTime<Utc>,
    since_hours: u64,
) -> Result<Summary> {
    let duration = Duration::try_hours(i64::try_from(since_hours).unwrap_or(i64::MAX))
        .context("--since-hours duration is too large")?;
    let cutoff = now
        .checked_sub_signed(duration)
        .context("--since-hours window is out of range")?;
    let mut summary = Summary::default();
    let entries = match fs::read_dir(heartbeat_dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(summary),
        result => result
            .with_context(|| format!("Reading ledger directory {}", heartbeat_dir.display()))?,
    };
    for entry in entries {
        let entry =
            entry.with_context(|| format!("Reading entry in {}", heartbeat_dir.display()))?;
        if !entry
            .file_name()
            .to_string_lossy()
            .ends_with(".blocks.jsonl")
        {
            continue;
        }
        let path = entry.path();
        if let Some((file, len)) = open_ledger(&path)? {
            collect_records(&read_tail(file, &path, len)?, cutoff, &mut summary);
        }
    }
    summary
        .records
        .sort_by_key(|record| std::cmp::Reverse(record.ts));
    Ok(summary)
}

/// Opens a ledger without following a symlink and without blocking on a FIFO,
/// then checks the opened handle, so a file swapped in after the directory
/// listing cannot hang the reader. `None` skips a symlink or non-regular file.
fn open_ledger(path: &Path) -> Result<Option<(File, u64)>> {
    let opened = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path);
    let file = match opened {
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
        result => result.with_context(|| format!("Opening ledger {}", path.display()))?,
    };
    let metadata = file
        .metadata()
        .with_context(|| format!("Inspecting ledger {}", path.display()))?;
    Ok(metadata.is_file().then_some((file, metadata.len())))
}

fn read_tail(mut file: File, path: &Path, len: u64) -> Result<Vec<u8>> {
    let offset = len.saturating_sub(MAX_BYTES);
    file.seek(SeekFrom::Start(offset))
        .with_context(|| format!("Seeking ledger {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .with_context(|| format!("Reading ledger {}", path.display()))?;
    if offset > 0 {
        // The bounded tail can start inside a record; discard that partial line.
        let start = bytes
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(bytes.len(), |index| index + 1);
        bytes.drain(..start);
    }
    Ok(bytes)
}

fn collect_records(bytes: &[u8], cutoff: DateTime<Utc>, summary: &mut Summary) {
    if bytes.is_empty() {
        return;
    }
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    for line in bytes.rsplit(|&byte| byte == b'\n').take(MAX_LINES) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > MAX_LINE_BYTES {
            summary.malformed += 1;
            continue;
        }
        match serde_json::from_slice::<BlockRecord>(line) {
            Ok(record) if record.ts >= cutoff => summary.records.push(record),
            Ok(_) => {}
            Err(_) => summary.malformed += 1,
        }
    }
}

pub(crate) fn render(summary: &Summary, by_rule: bool) -> String {
    if summary.records.is_empty() {
        let mut text = "no hook blocks recorded in the window".to_owned();
        if summary.malformed > 0 {
            text.push_str(&format!(" ({} malformed lines skipped)", summary.malformed));
        }
        return text;
    }
    let mut lines = if by_rule {
        render_groups(&summary.records)
    } else {
        summary
            .records
            .iter()
            .take(20)
            .map(|record| {
                format!(
                    "{} {} {} {}/{} {}",
                    record.ts.to_rfc3339(),
                    safe_field(&record.stage),
                    safe_field(&record.role),
                    safe_field(&record.guard),
                    safe_field(&record.rule),
                    subject(&record.subject)
                )
            })
            .collect()
    };
    if summary.malformed > 0 {
        lines.push(format!("{} malformed lines skipped", summary.malformed));
    }
    lines.join("\n")
}

fn render_groups(records: &[BlockRecord]) -> Vec<String> {
    let mut groups: BTreeMap<String, Vec<&BlockRecord>> = BTreeMap::new();
    for record in records {
        let name = format!("{}/{}", record.guard, record.rule);
        groups.entry(name).or_default().push(record);
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|(name_a, a), (name_b, b)| b.len().cmp(&a.len()).then(name_a.cmp(name_b)));
    let mut lines = Vec::new();
    for (name, records) in groups {
        let main = records
            .iter()
            .filter(|record| record.role == "main")
            .count();
        let subagent = records
            .iter()
            .filter(|record| record.role == "subagent")
            .count();
        lines.push(format!(
            "{} {} main={main} subagent={subagent}",
            records.len(),
            safe_field(&name)
        ));
        lines.extend(
            records
                .iter()
                .take(3)
                .map(|record| format!("  {}", subject(&record.subject))),
        );
    }
    lines
}

fn safe_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { '?' } else { ch })
        .collect()
}

fn subject(value: &str) -> String {
    safe_field(value).chars().take(120).collect()
}
