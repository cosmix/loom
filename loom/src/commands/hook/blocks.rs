//! Read and summarize the diagnostic ledgers written by shell guards.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

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
    let dir = ledger_dir(std::env::var_os("LOOM_WORK_DIR"))?;
    let shown = safe_field(&dir.display().to_string());
    println!("reading {shown}");
    match fs::metadata(&dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            println!("no ledger directory at {shown}");
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

/// The heartbeat directory under a set, non-empty `LOOM_WORK_DIR` taken verbatim
/// (non-UTF-8 included, as the hooks write it), else under `WorkDir::new(".")`.
pub(crate) fn ledger_dir(work_dir: Option<OsString>) -> Result<PathBuf> {
    match work_dir.filter(|root| !root.is_empty()) {
        Some(root) => Ok(PathBuf::from(root).join("heartbeat")),
        None => Ok(WorkDir::new(".")?.root().join("heartbeat")),
    }
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
    summary.records.sort_by(|a, b| {
        b.ts.cmp(&a.ts)
            .then_with(|| a.stage.cmp(&b.stage))
            .then_with(|| a.subject.cmp(&b.subject))
    });
    Ok(summary)
}

/// Opens a ledger without following a symlink and without blocking on a FIFO,
/// then checks the opened handle, so a file swapped in after the directory
/// listing cannot hang the reader. `None` skips a symlink, a non-regular file,
/// or any ledger that cannot be opened (a socket's ENXIO, EACCES, ...), so one
/// bad file never hides the others.
fn open_ledger(path: &Path) -> Result<Option<(File, u64)>> {
    let opened = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path);
    let file = match opened {
        Ok(file) => file,
        Err(error) => {
            if error.raw_os_error() != Some(libc::ELOOP) {
                let shown = safe_field(&path.display().to_string());
                eprintln!("skipping ledger {shown}: {error}");
            }
            return Ok(None);
        }
    };
    let metadata = file
        .metadata()
        .with_context(|| format!("Inspecting ledger {}", path.display()))?;
    Ok(metadata.is_file().then_some((file, metadata.len())))
}

fn read_tail(mut file: File, path: &Path, len: u64) -> Result<Vec<u8>> {
    // A bounded tail also reads the byte before its window: the window starts a
    // record exactly when that byte is a newline.
    let offset = len.saturating_sub(MAX_BYTES);
    let lead = u64::from(offset > 0);
    file.seek(SeekFrom::Start(offset - lead))
        .with_context(|| format!("Seeking ledger {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + lead)
        .read_to_end(&mut bytes)
        .with_context(|| format!("Reading ledger {}", path.display()))?;
    if lead > 0 {
        // Discard through the first newline: the leading byte, or a partial record.
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

/// Prints control characters and Unicode bidi format characters as `?`, so a
/// ledger line can neither replay terminal escapes nor reorder the report.
pub(crate) fn safe_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            let bidi = matches!(ch, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
            if ch.is_control() || bidi {
                '?'
            } else {
                ch
            }
        })
        .collect()
}

fn subject(value: &str) -> String {
    safe_field(value).chars().take(120).collect()
}
