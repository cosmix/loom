//! Render a pressure-test record and publish a replacement plan.

use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions, Permissions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Context, Result};

use crate::fs::knowledge::splice::fence_mask;
use crate::planning::sections::sections_of;

const MAX_PLAN_BYTES: u64 = 8 * 1024 * 1024;
const END_METADATA: &str = "<!-- END loom METADATA";

#[cfg(test)]
type BeforePublication = Box<dyn FnOnce(&Path)>;

#[cfg(test)]
thread_local! {
    static BEFORE_PUBLICATION: std::cell::RefCell<Option<BeforePublication>> =
        const { std::cell::RefCell::new(None) };
}

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

/// Whether the record was saved or a conflict was observed before publication.
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
    let scan_lines: Vec<&str> = lines
        .iter()
        .map(|line| line.trim_end_matches(['\n', '\r']))
        .collect();
    let fenced = fence_mask(&scan_lines);
    let document = sections_of(text);
    // Use the parsed metadata boundary when present. A marker inside a
    // fenced example cannot define the boundary, even if extraction saw it.
    let marker = document
        .metadata
        .as_ref()
        .and_then(|span| span.end_line.checked_sub(1))
        .filter(|&index| !fenced[index] && scan_lines[index].starts_with(END_METADATA))
        .or_else(|| {
            scan_lines
                .iter()
                .enumerate()
                .position(|(index, line)| !fenced[index] && line.starts_with(END_METADATA))
        })?;
    let section = document.headings.iter().find(|heading| {
        heading.level == 2 && heading.title == "Pressure tests" && heading.span.start_line > marker
    })?;
    let last = (section.span.start_line..section.span.end_line)
        .rev()
        .find(|&index| !scan_lines[index].trim().is_empty())?;
    Some(lines.iter().take(last + 1).map(|line| line.len()).sum())
}

/// Append a record through a synced sibling temp file, retaining the plan mode.
/// Missing, changed, replaced, and symlinked plans observed before publication
/// return [`RecordWrite::PlanMoved`]. External editors do not take Loom's lock:
/// an edit between the last comparison and rename can still be overwritten.
pub fn append_record(plan_path: &Path, line: &RecordLine) -> Result<RecordWrite> {
    if regular_metadata(plan_path)?.is_none() {
        return Ok(RecordWrite::PlanMoved);
    }
    let _plan_lock = crate::fs::locking::lock_plan(plan_path)?;
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
    let result = replace_plan(plan_path, &temp_path, &mut temp, &original, &text, &updated);
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

fn same_revision(current: &Metadata, original: &Metadata) -> bool {
    same_file(current, original)
        && current.len() == original.len()
        && current.mtime() == original.mtime()
        && current.mtime_nsec() == original.mtime_nsec()
        && current.ctime() == original.ctime()
        && current.ctime_nsec() == original.ctime_nsec()
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
    if !same_revision(&current, original) {
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
    if !same_revision(
        &file.metadata().context("Failed to re-inspect open plan")?,
        original,
    ) {
        return Ok(None);
    }
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
    snapshot: &str,
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
    #[cfg(test)]
    BEFORE_PUBLICATION.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook(plan_path);
        }
    });
    if !still_matches(plan_path, original, snapshot)? {
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

/// This detects changed bytes but cannot make the later rename conditional.
/// External editors do not take Loom's advisory lock.
fn still_matches(plan_path: &Path, original: &Metadata, snapshot: &str) -> Result<bool> {
    let Some(current) = regular_metadata(plan_path)? else {
        return Ok(false);
    };
    if !same_file(&current, original) {
        return Ok(false);
    }
    let Some((_source, live)) = read_plan(plan_path, original)? else {
        return Ok(false);
    };
    Ok(live == snapshot)
}

#[cfg(test)]
mod tests;
