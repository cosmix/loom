//! Render and atomically append the pressure-test record to a plan.

use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions, Permissions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Context, Result};

use crate::fs::knowledge::splice::fence_mask;

const MAX_PLAN_BYTES: u64 = 8 * 1024 * 1024;
const END_METADATA: &str = "<!-- END loom METADATA";

/// The models, rounds, and review outcome of one pressure-test run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLine {
    pub date: chrono::NaiveDate,
    pub claude_model: String,
    /// One entry per round, in round order; an empty entry means unknown.
    pub codex_models: Vec<String>,
    pub rounds: u32,
    /// One-based rounds whose Codex report was absent at `/address`.
    pub missing_rounds: Vec<u32>,
}

impl RecordLine {
    /// Render the Markdown list item, without its terminating newline.
    pub fn render(&self) -> String {
        let models: Vec<&str> = self
            .codex_models
            .iter()
            .map(|model| if model.is_empty() { "unknown" } else { model })
            .collect();
        let codex = if self.codex_models.windows(2).all(|pair| pair[0] == pair[1]) {
            models.first().copied().unwrap_or("unknown").to_owned()
        } else {
            models.join(" / ")
        };
        let outcome = if self.missing_rounds.is_empty() {
            "review addressed".to_owned()
        } else {
            let rounds = self
                .missing_rounds
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("Codex review missing in round {rounds}")
        };
        let unit = if self.rounds == 1 { "round" } else { "rounds" };
        format!(
            "- {}: Claude {}, Codex {codex}; {} {unit}, {outcome}",
            self.date.format("%Y-%m-%d"),
            self.claude_model,
            self.rounds
        )
    }
}

/// Whether the record was saved or the plan no longer occupies its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordWrite {
    Written,
    PlanMoved,
}

/// Place a rendered record after the metadata, preserving the existing text.
pub fn append_to_text(text: &str, rendered: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut result = String::with_capacity(text.len() + rendered.len() + 32);
    if let Some(offset) = record_offset(text) {
        let (before, after) = text.split_at(offset);
        result.push_str(before);
        if !before.ends_with('\n') {
            result.push_str(newline);
        }
        result.push_str(rendered);
        result.push_str(newline);
        result.push_str(after);
    } else {
        result.push_str(text);
        if !text.ends_with('\n') {
            result.push_str(newline);
        }
        result.push_str(newline);
        result.push_str("## Pressure tests");
        result.push_str(newline);
        result.push_str(newline);
        result.push_str(rendered);
        result.push_str(newline);
    }
    result
}

fn record_offset(text: &str) -> Option<usize> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    // Headings inside fenced code are not headings to the reader
    // (`planning::pressure::records`), so they must not place the record either.
    let fenced = fence_mask(&lines);
    let mut starts = Vec::with_capacity(lines.len());
    let mut position = 0;
    for line in &lines {
        starts.push(position);
        position += line.len();
    }
    // A fenced example of the marker is not the marker either.
    let marker = (0..lines.len())
        .rposition(|index| !fenced[index] && lines[index].starts_with(END_METADATA))?;
    let mut insertion = None;
    for (index, line) in lines.iter().enumerate().skip(marker + 1) {
        let is_heading = !fenced[index];
        if insertion.is_some() && is_heading && (line.starts_with("# ") || line.starts_with("## "))
        {
            break;
        }
        let content = line.trim_end();
        if insertion.is_some() {
            if !content.is_empty() {
                insertion = Some(starts[index] + line.len());
            }
        } else if is_heading && content == "## Pressure tests" {
            insertion = Some(starts[index] + line.len());
        }
    }
    insertion
}

/// Append a record through a synced sibling temp file, retaining the plan mode.
/// Missing, replaced, and symlinked plans return [`RecordWrite::PlanMoved`].
pub fn append_record(plan_path: &Path, line: &RecordLine) -> Result<RecordWrite> {
    let Some(original) = regular_metadata(plan_path)? else {
        return Ok(RecordWrite::PlanMoved);
    };
    // Keep the source inode open until replacement so it cannot be recycled.
    let Some((_source, text)) = read_plan(plan_path, &original)? else {
        return Ok(RecordWrite::PlanMoved);
    };
    let updated = append_to_text(&text, &line.render());
    let temp_path = temp_path(plan_path)?;
    let mut temp = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(original.permissions().mode() & 0o7777)
        .open(&temp_path)
        .with_context(|| format!("Failed to create record temp file: {}", temp_path.display()))?;
    let result = replace_plan(plan_path, &temp_path, &mut temp, &original, &updated);
    drop(temp);
    if !matches!(&result, Ok(RecordWrite::Written)) {
        // Only remove a temp file we successfully created, never a colliding path.
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn regular_metadata(path: &Path) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_file().then_some(metadata)),
        Err(error)
            if error.kind() == ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ENOTDIR) =>
        {
            Ok(None)
        }
        Err(error) => {
            Err(error).with_context(|| format!("Failed to inspect plan: {}", path.display()))
        }
    }
}

fn same_file(current: &Metadata, original: &Metadata) -> bool {
    current.is_file() && current.ino() == original.ino() && current.dev() == original.dev()
}

fn read_plan(path: &Path, original: &Metadata) -> Result<Option<(File, String)>> {
    let mut file = match OpenOptions::new()
        .read(true)
        // Avoid following a swapped symlink or blocking on a swapped FIFO.
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error)
            if error.kind() == ErrorKind::NotFound
                || matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) =>
        {
            return Ok(None);
        }
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to open plan: {}", path.display()));
        }
    };
    let current = file.metadata().context("Failed to inspect open plan")?;
    if !same_file(&current, original) {
        return Ok(None);
    }
    ensure!(
        current.len() <= MAX_PLAN_BYTES,
        "Plan exceeds the 8 MiB read limit"
    );
    let mut text = String::new();
    (&mut file)
        .take(MAX_PLAN_BYTES + 1)
        .read_to_string(&mut text)
        .with_context(|| format!("Failed to read plan: {}", path.display()))?;
    ensure!(
        text.len() as u64 <= MAX_PLAN_BYTES,
        "Plan exceeds the 8 MiB read limit"
    );
    Ok(Some((file, text)))
}

fn temp_path(plan_path: &Path) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock predates the Unix epoch")?
        .as_nanos();
    let mut name = OsString::from(".");
    name.push(
        plan_path
            .file_name()
            .context("Plan path has no file name")?,
    );
    name.push(format!(".loom-record-{}-{nanos}", std::process::id()));
    Ok(plan_path.with_file_name(name))
}

fn replace_plan(
    plan_path: &Path,
    temp_path: &Path,
    temp: &mut File,
    original: &Metadata,
    text: &str,
) -> Result<RecordWrite> {
    temp.write_all(text.as_bytes())
        .with_context(|| format!("Failed to write record temp file: {}", temp_path.display()))?;
    // Apply permissions after writing: writing can clear setuid/setgid bits.
    temp.set_permissions(Permissions::from_mode(
        original.permissions().mode() & 0o7777,
    ))
    .context("Failed to preserve plan permissions")?;
    temp.sync_all().context("Failed to sync record temp file")?;
    let Some(current) = regular_metadata(plan_path)? else {
        return Ok(RecordWrite::PlanMoved);
    };
    if !same_file(&current, original) {
        return Ok(RecordWrite::PlanMoved);
    }
    fs::rename(temp_path, plan_path)
        .with_context(|| format!("Failed to replace plan: {}", plan_path.display()))?;
    // The plan already holds the record; a directory that cannot be opened or
    // synced (for example mode 0311) only costs crash durability, so this is
    // best effort like `fs::locking::atomic_write`.
    let parent = plan_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(RecordWrite::Written)
}

#[cfg(test)]
mod tests;
