//! Plan-level `provision` entries: the executor that runs them in a stage
//! worktree and the `config.toml` snapshot `loom init` takes of them.
//!
//! The daemon runs the snapshot, never the plan file: the plan file is merged
//! content a sandboxed stage may edit, while the work directory is a control path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

use crate::context::untrusted::flatten_char;
use crate::fs::work_dir::{read_config, update_config, write_plan_sandbox};
use crate::plan::schema::{LoomConfig, ProvisionEntry};
use crate::verify::criteria::{run_provision_with_timeout, CommandSpec, CriterionResult};

/// How long one provision command may run.
pub const PROVISION_TIMEOUT: Duration = Duration::from_secs(600);

/// Section of the work directory's `config.toml` that holds the provision entries
/// `loom init` copied from the plan. The gate reads only this, never the plan file.
const PROVISION_SECTION: &str = "plan_provision";

/// Most output lines a failure reason keeps, and the most characters of each.
const DETAIL_LINES: usize = 10;
const DETAIL_LINE_CHARS: usize = 300;

/// The `[plan_provision]` table: a TOML section is a table, so the entries sit
/// under one key.
#[derive(Debug, Serialize, Deserialize)]
struct ProvisionSnapshot {
    entries: Vec<ProvisionEntry>,
}

/// Run the plan's provision entries in `worktree`, in order, stopping at the first
/// failure. `Err` is the stage's block reason.
pub fn run_provision(entries: &[ProvisionEntry], worktree: &Path) -> Result<(), String> {
    entries
        .iter()
        .try_for_each(|entry| run_entry(entry, worktree))
}

/// Persist the plan-level snapshots `loom init` takes: the sandbox (as today, through
/// `crate::fs::work_dir::write_plan_sandbox`) and the provision entries.
pub fn persist_plan_snapshots(work_dir: &Path, loom: &LoomConfig) -> Result<()> {
    write_plan_sandbox(work_dir, &loom.sandbox)?;
    write_provision_snapshot(work_dir, &loom.provision)
}

/// Replace the provision snapshot; no entries removes the section.
pub fn write_provision_snapshot(work_dir: &Path, entries: &[ProvisionEntry]) -> Result<()> {
    let snapshot = ProvisionSnapshot {
        entries: entries.to_vec(),
    };
    let rendered = toml::to_string_pretty(&BTreeMap::from([(PROVISION_SECTION, snapshot)]))
        .context("Failed to render the [plan_provision] section")?;
    let rendered: DocumentMut = rendered
        .parse()
        .context("Failed to re-parse the rendered [plan_provision] section")?;
    update_config(work_dir, |doc| {
        match rendered
            .get(PROVISION_SECTION)
            .filter(|_| !entries.is_empty())
        {
            Some(item) => doc.insert(PROVISION_SECTION, item.clone()),
            None => doc.remove(PROVISION_SECTION),
        };
        Ok(())
    })
}

/// The snapshot's entries; a missing section is no entries.
pub fn read_provision_snapshot(work_dir: &Path) -> Result<Vec<ProvisionEntry>> {
    let doc = read_config(work_dir)?;
    let value: toml::Value =
        toml::from_str(&doc.to_string()).context("Failed to parse config.toml as TOML")?;
    let Some(section) = value.get(PROVISION_SECTION).cloned() else {
        return Ok(Vec::new());
    };
    let snapshot: ProvisionSnapshot = section
        .try_into()
        .context("Failed to deserialize the [plan_provision] section")?;
    Ok(snapshot.entries)
}

/// The block reason for a failed entry; `working_dir` is as the plan wrote it, made
/// [`display_safe`] like the command.
fn failure(entry: &ProvisionEntry, detail: &str) -> String {
    format!(
        "provision `{}` in `{}` failed: {detail}",
        display_safe(&entry.command),
        display_safe(&entry.working_dir)
    )
}

/// `text` with every control, line-breaking, format and bidi character made a space,
/// cut to [`DETAIL_LINE_CHARS`] characters. A block reason is printed to the
/// operator's terminal and kept as the stage's close reason and failure evidence, and
/// its parts (host-command output, file names a stage can choose, the plan's command)
/// would otherwise carry ANSI/OSC sequences and line breaks into all three.
pub(crate) fn display_safe(text: &str) -> String {
    text.chars()
        .map(flatten_char)
        .take(DETAIL_LINE_CHARS)
        .collect()
}

fn run_entry(entry: &ProvisionEntry, worktree: &Path) -> Result<(), String> {
    let dir = resolve_dir(entry, worktree)?;
    // The assigned worktree is the boundary; an entry may use sibling workspace
    // packages while executing from its validated subdirectory.
    let command = format!(
        "cd -- {} && {}",
        shell_escape::escape(dir.to_string_lossy()),
        entry.command
    );
    let result =
        run_provision_with_timeout(&CommandSpec::shell(command), worktree, PROVISION_TIMEOUT)
            .map_err(|e| failure(entry, &tail_lines(&format!("{e:#}"))))?;
    if result.success {
        Ok(())
    } else {
        Err(failure(entry, &failure_detail(&result)))
    }
}

/// `<worktree>/<working_dir>`, canonical and inside the canonical worktree; a
/// missing directory or a symlink pointing out is an `Err` before anything runs.
fn resolve_dir(entry: &ProvisionEntry, worktree: &Path) -> Result<PathBuf, String> {
    let root = worktree
        .canonicalize()
        .map_err(|e| failure(entry, &format!("cannot resolve the worktree: {e}")))?;
    let dir = root
        .join(&entry.working_dir)
        .canonicalize()
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| failure(entry, "the directory does not exist in the worktree"))?;
    if dir.starts_with(&root) {
        Ok(dir)
    } else {
        Err(failure(
            entry,
            "the directory resolves outside the worktree",
        ))
    }
}

/// Why a command that ran did not succeed: a timeout, the tail of stderr, else of
/// stdout, else the exit code.
fn failure_detail(result: &CriterionResult) -> String {
    if result.timed_out {
        return format!("timed out after {} s", PROVISION_TIMEOUT.as_secs());
    }
    [&result.stderr, &result.stdout]
        .into_iter()
        .map(|text| tail_lines(text))
        .find(|tail| !tail.is_empty())
        .unwrap_or_else(|| match result.exit_code {
            Some(code) => format!("exit {code}"),
            None => "killed by a signal".to_string(),
        })
}

/// The last [`DETAIL_LINES`] lines of `text` that hold more than control or blank
/// characters, each made [`display_safe`], joined with `\n`.
fn tail_lines(text: &str) -> String {
    let blank = |ch: char| flatten_char(ch) == ' ';
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim_end_matches(blank))
        .filter(|line| !line.chars().all(blank))
        .collect();
    let kept = &lines[lines.len().saturating_sub(DETAIL_LINES)..];
    kept.iter()
        .copied()
        .map(display_safe)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "provision_tests.rs"]
mod tests;
