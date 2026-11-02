//! Whether a stage's agent is inside a tool call that has not returned.
//!
//! The heartbeat is refreshed after a tool call returns, so one long call (a
//! subagent's `cargo test`, a forwarded job) leaves it silent while the stage
//! is healthy. The transcripts say what the heartbeat cannot: the main agent's
//! own, or any subagent's, ends in an assistant `tool_use` with no result yet.

use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};

use crate::commands::subagents::resolve::list_agent_files;
use crate::commands::subagents::{open_tool_call, OpenToolCall};
use crate::fs::safe_read::open_regular_no_follow;

/// Where a heartbeat's transcript sits under the Claude projects root.
struct TranscriptHome {
    /// The canonical projects root; every open is made beneath it.
    root: PathBuf,
    /// The transcript's directory, relative to `root`.
    directory: PathBuf,
    /// `<session-uuid>.jsonl`
    file_name: OsString,
}

/// The call the stage's agent or one of its subagents has open, if any.
///
/// `transcript` comes from a heartbeat file the session writes, so it is
/// treated as a claim: it must be an absolute `.jsonl` path whose directory
/// resolves under `projects_root`, and each file is opened without following a
/// symlink at any component, as a single-link regular file. Anything else, and
/// any read failure, answers `None`: the status payload never errors on it.
///
/// Subagent transcripts last written before `progress_at` are finished work
/// and are not read.
pub(super) fn open_tool(
    transcript: &str,
    progress_at: DateTime<Utc>,
    projects_root: &Path,
) -> Option<OpenToolCall> {
    let home = locate(Path::new(transcript), projects_root)?;
    let main = home.directory.join(&home.file_name);
    if let Some(call) = open_file(&home.root, &main).and_then(|mut file| open_tool_call(&mut file))
    {
        return Some(call);
    }
    open_in_subagents(&home, progress_at)
}

fn locate(path: &Path, projects_root: &Path) -> Option<TranscriptHome> {
    let well_formed = path.is_absolute()
        && path.extension().is_some_and(|ext| ext == "jsonl")
        && !path
            .components()
            .any(|component| component == Component::ParentDir);
    if !well_formed {
        return None;
    }
    let root = fs::canonicalize(projects_root).ok()?;
    let directory = fs::canonicalize(path.parent()?).ok()?;
    let directory = directory.strip_prefix(&root).ok()?.to_path_buf();
    Some(TranscriptHome {
        root,
        directory,
        file_name: path.file_name()?.to_owned(),
    })
}

fn open_file(root: &Path, relative: &Path) -> Option<File> {
    open_regular_no_follow(root, relative.to_str()?, libc::O_RDONLY)
        .ok()
        .flatten()
}

/// Subagents live in `<session-uuid>/subagents/agent-*.jsonl` beside the main
/// transcript, the layout `loom subagents` reads.
fn open_in_subagents(home: &TranscriptHome, progress_at: DateTime<Utc>) -> Option<OpenToolCall> {
    let stem = Path::new(&home.file_name).file_stem()?;
    let directory = home.directory.join(stem).join("subagents");
    let since = SystemTime::from(progress_at);
    list_agent_files(&home.root.join(&directory))
        .into_iter()
        .filter_map(|path| open_file(&home.root, &directory.join(path.file_name()?)))
        .filter(|file| written_since(file, since))
        .find_map(|mut file| open_tool_call(&mut file))
}

fn written_since(file: &File, since: SystemTime) -> bool {
    file.metadata()
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified >= since)
}
