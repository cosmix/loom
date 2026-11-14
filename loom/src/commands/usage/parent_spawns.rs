//! Attribution for transcripts the hook ledgers do not cover: the stage from a
//! worktree project slug, and a subagent's requested model from its sidecar or
//! from the spawn `tool_use` in the parent transcript. Every fallback applies
//! only to a field the ledger join left `None`, so the ledger stays
//! authoritative.

use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::agent_sidecar::{read_sidecar, Sidecar};
use super::discovery::{stage_from_project_slug, DiscoveredFile};
use super::transcript::{Scope, Transcript};
use crate::fs::safe_read::open_regular_no_follow;
use crate::fs::work_dir::WorkDir;
use crate::provider_data::resolve::project_slug;
use crate::tokens::tail::next_line;

/// Longest parent-transcript line parsed. Longer lines are skipped unread.
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

/// What a spawn records when its input names no model.
const DEFAULT_MODEL: &str = "default";

/// Spawn `tool_use` ids of each parent transcript, each parent indexed at most
/// once per run.
pub(super) struct ParentSpawns {
    repo_slug: Option<String>,
    /// Parent transcript path -> (`tool_use` id -> requested model).
    parents: HashMap<PathBuf, HashMap<String, String>>,
}

impl ParentSpawns {
    /// Stores the project slug of the repository holding `work_dir`, `None`
    /// when `work_dir` is `None` or sits too shallow to have one. The path is
    /// canonicalized first: inside a worktree `.loom/work` is a symlink to the
    /// main repository's, and the un-resolved path would name the worktree.
    pub(super) fn new(work_dir: Option<&Path>) -> Self {
        let repo_slug = work_dir.and_then(|dir| {
            let resolved = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
            WorkDir::new(&resolved).ok()?.repo_root().map(project_slug)
        });
        Self {
            repo_slug,
            parents: HashMap::new(),
        }
    }

    /// Slug stage fallback for every scope, then, for subagents, the agent type
    /// and requested model from one read of the sidecar. A field the ledger
    /// already set is left alone and a sidecar nobody needs is not read.
    pub(super) fn fill_fallbacks(&mut self, transcript: &mut Transcript, file: &DiscoveredFile) {
        if transcript.stage_id.is_none() {
            transcript.stage_id =
                stage_from_project_slug(&file.project_slug, self.repo_slug.as_deref())
                    .map(str::to_owned);
        }
        if file.scope != Scope::Subagent
            || (transcript.agent_type.is_some() && transcript.requested_model.is_some())
        {
            return;
        }
        let Some(sidecar) = read_sidecar(&file.path) else {
            return;
        };
        if transcript.agent_type.is_none() {
            transcript.agent_type = sidecar.agent_type.clone();
            transcript.agent_type_from_sidecar = transcript.agent_type.is_some();
        }
        if transcript.requested_model.is_none() {
            transcript.requested_model = self.requested_model(file, sidecar);
        }
    }

    /// The sidecar's `model`; else the parent spawn's `input.model`, or
    /// `default` when that spawn names none; else `None`.
    fn requested_model(&mut self, file: &DiscoveredFile, sidecar: Sidecar) -> Option<String> {
        if sidecar.model.is_some() {
            return sidecar.model;
        }
        let tool_use_id = sidecar.tool_use_id?;
        // <project>/<session>/subagents/agent-<id>.jsonl
        let project_dir = file.path.parent()?.parent()?.parent()?;
        let parent = project_dir.join(format!("{}.jsonl", file.session_id));
        let spawns = self
            .parents
            .entry(parent)
            .or_insert_with(|| index_parent(project_dir, &file.session_id));
        spawns.get(&tool_use_id).cloned()
    }
}

/// Every `Agent` or `Task` spawn in the parent transcript, streamed line by
/// line: real parents exceed 20 MB, so they are never read whole. A missing,
/// symlinked or unreadable parent indexes nothing.
fn index_parent(project_dir: &Path, session_id: &str) -> HashMap<String, String> {
    let mut spawns = HashMap::new();
    let relpath = format!("{session_id}.jsonl");
    let Ok(Some(file)) = open_regular_no_follow(project_dir, &relpath, libc::O_RDONLY) else {
        return spawns;
    };
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    while let Ok(Some((_, cut))) = next_line(&mut reader, &mut line, MAX_LINE_BYTES) {
        if !cut && contains(&line, b"\"tool_use\"") {
            collect_spawns(&line, &mut spawns);
        }
    }
    spawns
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn collect_spawns(line: &[u8], spawns: &mut HashMap<String, String>) {
    let Ok(row) = serde_json::from_slice::<Value>(line) else {
        return;
    };
    let Some(blocks) = row.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for block in blocks {
        let is_spawn = block.get("type").and_then(Value::as_str) == Some("tool_use")
            && matches!(
                block.get("name").and_then(Value::as_str),
                Some("Agent" | "Task")
            );
        let Some(id) = block.get("id").and_then(Value::as_str).filter(|_| is_spawn) else {
            continue;
        };
        let model = block
            .pointer("/input/model")
            .and_then(Value::as_str)
            .filter(|model| !model.is_empty())
            .unwrap_or(DEFAULT_MODEL);
        spawns.insert(id.to_owned(), model.to_owned());
    }
}

#[cfg(test)]
#[path = "parent_spawns_tests.rs"]
mod tests;
