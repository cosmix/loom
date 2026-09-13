//! The text a Bash tool call produced, as the relay reads it: the inline
//! `tool_response`/`tool_result` fields, plus the file Claude Code persisted a
//! large output to, but only a file the harness itself could have written.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

use crate::fs::safe_read::open_regular_no_follow;
use crate::relay::MAX_PERSISTED_OUTPUT_BYTES;

/// The current payload shape first, then the older one.
const RESPONSE_KEYS: [&str; 2] = ["tool_response", "tool_result"];
const OUTPUT_FIELDS: [&str; 3] = ["stdout", "stderr", "output"];
/// The inline wrapper text naming a persisted output file.
const SAVED_TO: &str = "Full output saved to: ";
/// The harness's own persistence directory inside a session's project dir.
const TOOL_RESULTS: &str = "tool-results";

pub(super) struct CollectedOutput {
    /// Inline output, then the persisted file's content when it validated.
    pub(super) text: String,
    /// Why a persisted output file the payload named was not read.
    pub(super) persisted_refusal: Option<String>,
}

/// Every inline output string in `payload`, newline-joined.
pub(super) fn inline_text(payload: &Value) -> String {
    RESPONSE_KEYS
        .iter()
        .filter_map(|key| payload.get(key))
        .flat_map(|response| {
            OUTPUT_FIELDS
                .iter()
                .filter_map(move |field| response.get(field)?.as_str())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Inline output plus the persisted output file, when one is named and it
/// validates under `projects_root`.
pub(super) fn collect(payload: &Value, projects_root: &Path) -> CollectedOutput {
    let mut text = inline_text(payload);
    let mut persisted_refusal = None;
    if let Some(path) = persisted_path(payload, &text) {
        match read_persisted(&path, projects_root) {
            Ok(content) => {
                text.push('\n');
                text.push_str(&content);
            }
            Err(error) => {
                persisted_refusal = Some(format!(
                    "LOOM relay: the persisted output file {} was not read ({error:#}), so no \
                     request in it was relayed.",
                    path.display()
                ));
            }
        }
    }
    CollectedOutput {
        text,
        persisted_refusal,
    }
}

/// The structured `persistedOutputPath` field, else the path the inline
/// "Full output saved to:" wrapper names. The command's own output can forge
/// either; [`read_persisted`] is what makes reading it safe.
fn persisted_path(payload: &Value, text: &str) -> Option<PathBuf> {
    RESPONSE_KEYS
        .iter()
        .find_map(|key| payload.get(key)?.get("persistedOutputPath")?.as_str())
        .or_else(|| {
            text.lines()
                .find_map(|line| line.split_once(SAVED_TO).map(|(_, path)| path.trim_end()))
        })
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// Read a persisted output file only when the harness could have written it.
///
/// `~/.claude/projects/` is write-denied to a sandboxed session, so a file
/// that really lies there was written by Claude Code. These checks keep a
/// forged path from pointing anywhere else: absolute, no `..` component, a
/// directory that canonicalizes under the canonical projects root with a
/// `tool-results` component, and the file itself opened without following a
/// link at any component, as a single-link regular file within the size cap
/// (`mistakes/completion-broker-credential.md`).
pub(super) fn read_persisted(path: &Path, projects_root: &Path) -> Result<String> {
    if !path.is_absolute() {
        bail!("the path is not absolute");
    }
    if path.components().any(|part| part == Component::ParentDir) {
        bail!("the path has a `..` component");
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        bail!("the path names no file");
    };
    let root = fs::canonicalize(projects_root)
        .with_context(|| format!("{} does not resolve", projects_root.display()))?;
    let directory = fs::canonicalize(parent).context("its directory does not resolve")?;
    let inside = directory
        .strip_prefix(&root)
        .map_err(|_| anyhow!("it does not resolve under {}", root.display()))?;
    if !inside
        .components()
        .any(|part| part.as_os_str() == TOOL_RESULTS)
    {
        bail!("it is not inside a {TOOL_RESULTS} directory");
    }
    let relative = inside.join(name);
    let relative = relative.to_str().context("the path is not UTF-8")?;
    let file = open_regular_no_follow(&root, relative, libc::O_RDONLY)?
        .context("the file does not exist")?;
    read_capped(file)
}

fn read_capped(file: fs::File) -> Result<String> {
    let size = file
        .metadata()
        .context("the file could not be inspected")?
        .len();
    if size > MAX_PERSISTED_OUTPUT_BYTES {
        bail!("it is {size} bytes, above the {MAX_PERSISTED_OUTPUT_BYTES}-byte cap");
    }
    let mut bytes = Vec::new();
    file.take(MAX_PERSISTED_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("the file could not be read")?;
    if bytes.len() as u64 > MAX_PERSISTED_OUTPUT_BYTES {
        bail!("it grew past the {MAX_PERSISTED_OUTPUT_BYTES}-byte cap while being read");
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
#[path = "tests_output.rs"]
mod tests;
