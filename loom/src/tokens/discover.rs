//! The files each stage's agents write: the Claude transcript of every stage
//! session, the subagent transcripts beside it, and the Codex rollouts of the
//! stage's forwarded jobs.

use super::model::AgentKind;
use crate::context::untrusted::inline_safe;
use crate::fs::work_dir::WorkDir;
use crate::models::forward_receipt::{is_safe_id, load_receipts, receipts_path};
use crate::provider_data::codex_discovery::{codex_root, collect_jsonl, CodexDiscovery};
use crate::provider_data::ledger::StartedAgentTypeIndex;
use crate::provider_data::resolve::{agent_id_from_path, list_agent_files};
use crate::provider_data::sessions::load_all_sessions;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub(super) const MAIN_PREFIX: &str = "main-";
pub(super) const CODEX_PREFIX: &str = "codex-";
/// A Codex thread whose rollout was not found is looked for again after this.
pub(super) const MISS_RETRY_SECS: i64 = 600;
/// Length of the `YYYY-MM-DDThh-mm-ss` timestamp in a rollout file name.
const ROLLOUT_TIMESTAMP_LEN: usize = 19;

/// One file to read, and the agent its usage belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Source {
    pub(super) stage_id: String,
    /// The agent's checkpoint key: `main-<session uuid>`, the subagent id, or
    /// `codex-<thread id>`.
    pub(super) key: String,
    pub(super) kind: AgentKind,
    pub(super) path: String,
    pub(super) agent_type: Option<String>,
}

/// Where a Codex thread's rollout was found, or when it was last looked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ThreadLookup {
    pub(super) rollout: Option<String>,
    pub(super) checked_at: i64,
}

/// Every source of every stage. `threads` caches rollout lookups between
/// passes; the Codex sessions root (`codex_sessions_root`, else
/// `<codex root>/sessions`) is walked at most once, and only for threads not
/// looked up yet or missed more than [`MISS_RETRY_SECS`] ago. A file reached
/// twice (two session records naming one transcript) is read for one agent.
pub(super) fn discover(
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    threads: &mut BTreeMap<String, ThreadLookup>,
    now: i64,
) -> Result<Vec<Source>> {
    discover_with(
        work_root,
        codex_sessions_root,
        threads,
        now,
        crate::codex_lifecycle::authorized_threads,
    )
}

fn discover_with(
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    threads: &mut BTreeMap<String, ThreadLookup>,
    now: i64,
    authorized: impl FnOnce(&Path, &[crate::models::session::Session]) -> Vec<(String, String)>,
) -> Result<Vec<Source>> {
    let mut sessions = load_all_sessions(&WorkDir::new(work_root)?)?;
    sessions.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
    let mut sources = claude_sources(work_root, &sessions);
    let stages: BTreeSet<&str> = sessions
        .iter()
        .filter_map(|s| s.stage_id.as_deref())
        .filter(|stage| is_safe_id(stage))
        .collect();
    let mut stage_threads = stage_threads(work_root, &stages);
    stage_threads.extend(authorized(work_root, &sessions));
    let mut seen = HashSet::new();
    stage_threads.retain(|(_, thread)| seen.insert(thread.clone()));
    resolve_threads(threads, &stage_threads, codex_sessions_root, now);
    let codex: Vec<Source> = stage_threads
        .into_iter()
        .filter_map(|(stage_id, thread)| codex_source(stage_id, thread, threads))
        .collect();
    sources.extend(codex);
    let mut paths = HashSet::new();
    let mut keys = HashSet::new();
    sources.retain(|s| {
        paths.insert(s.path.clone()) && keys.insert((s.stage_id.clone(), s.key.clone()))
    });
    Ok(sources)
}

/// Main and subagent transcripts of every session record with a safe stage id
/// and a transcript path, oldest session first. A handed-off stage has several
/// records; each counts.
fn claude_sources(work_root: &Path, sessions: &[crate::models::session::Session]) -> Vec<Source> {
    let index = StartedAgentTypeIndex::load(Some(work_root));
    let mut sources = Vec::new();
    for session in sessions {
        let (Some(stage_id), Some(transcript)) = (
            session.stage_id.as_deref(),
            session.transcript_path.as_deref(),
        ) else {
            continue;
        };
        if is_safe_id(stage_id) {
            session_sources(stage_id, transcript, &index, &mut sources);
        }
    }
    sources
}

/// One session's main transcript and the `agent-<id>.jsonl` files under
/// `<transcript dir>/<stem>/subagents/`. Files whose stem or agent id fails
/// `is_safe_id` are skipped.
fn session_sources(
    stage_id: &str,
    transcript: &str,
    index: &StartedAgentTypeIndex,
    sources: &mut Vec<Source>,
) {
    let path = Path::new(transcript);
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    if !is_safe_id(stem) {
        return;
    }
    sources.push(Source {
        stage_id: stage_id.to_owned(),
        key: format!("{MAIN_PREFIX}{stem}"),
        kind: AgentKind::Main,
        path: transcript.to_owned(),
        agent_type: None,
    });
    for file in list_agent_files(&path.with_file_name(stem).join("subagents")) {
        let agent_id = agent_id_from_path(&file);
        let Some(file_path) = file.to_str().filter(|_| is_safe_id(&agent_id)) else {
            continue;
        };
        let agent_type = index
            .get_metadata(&agent_id, stem)
            .map(|metadata| inline_safe(&metadata.agent_type));
        sources.push(Source {
            stage_id: stage_id.to_owned(),
            key: agent_id,
            kind: AgentKind::Subagent,
            path: file_path.to_owned(),
            agent_type,
        });
    }
}

/// `(stage id, thread id)` for every safe `codex_thread_id` in each stage's
/// forward receipts. A receipts path that is not a regular file is skipped
/// before `load_receipts` opens it, so a FIFO there cannot block the pass.
fn stage_threads(work_root: &Path, stages: &BTreeSet<&str>) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for stage_id in stages {
        let Ok(path) = receipts_path(work_root, stage_id) else {
            continue;
        };
        if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
            continue;
        }
        let Ok(load) = load_receipts(&path) else {
            continue;
        };
        let threads = load.receipts.into_iter().filter_map(|r| r.codex_thread_id);
        for thread in threads.filter(|thread| is_safe_id(thread)) {
            pairs.push(((*stage_id).to_owned(), thread));
        }
    }
    pairs
}

/// Looks the due threads of `stage_threads` up in one walk of the sessions
/// root. A rollout is the file `rollout-<timestamp>-<thread id>.jsonl` whose
/// whole thread id matches; a thread without one is recorded as a miss at
/// `now`.
fn resolve_threads(
    threads: &mut BTreeMap<String, ThreadLookup>,
    stage_threads: &[(String, String)],
    sessions_root: Option<&Path>,
    now: i64,
) {
    let due: BTreeSet<&str> = stage_threads
        .iter()
        .map(|(_, thread)| thread.as_str())
        .filter(|thread| {
            threads.get(*thread).is_none_or(|lookup| {
                // `checked_at` comes from the checkpoint file, so the difference saturates.
                lookup.rollout.is_none() && now.saturating_sub(lookup.checked_at) >= MISS_RETRY_SECS
            })
        })
        .collect();
    if due.is_empty() {
        return;
    }
    let rollouts = rollout_files(sessions_root);
    let by_thread = rollouts_by_thread(&rollouts);
    for thread in due {
        let lookup = ThreadLookup {
            rollout: by_thread.get(thread).map(|path| (*path).to_owned()),
            checked_at: now,
        };
        threads.insert(thread.to_owned(), lookup);
    }
}

/// The first rollout path of each thread id the files name.
fn rollouts_by_thread(rollouts: &[PathBuf]) -> HashMap<&str, &str> {
    let mut by_thread = HashMap::new();
    for path in rollouts {
        if let (Some(thread), Some(path)) = (rollout_thread_id(path), path.to_str()) {
            by_thread.entry(thread).or_insert(path);
        }
    }
    by_thread
}

/// The thread id of a rollout file named
/// `rollout-<YYYY-MM-DDThh-mm-ss>-<thread id>.jsonl`. The fixed-width
/// timestamp is what separates the thread id from the rest of the name: the
/// id itself contains hyphens, so matching on a `-<id>.jsonl` suffix would
/// also accept a short id equal to the last segment of another thread's id.
fn rollout_thread_id(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    let (timestamp, thread) = rest.split_at_checked(ROLLOUT_TIMESTAMP_LEN)?;
    if !is_rollout_timestamp(timestamp) {
        return None;
    }
    thread.strip_prefix('-').filter(|thread| !thread.is_empty())
}

fn is_rollout_timestamp(text: &str) -> bool {
    text.len() == ROLLOUT_TIMESTAMP_LEN
        && text.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 | 13 | 16 => byte == b'-',
            10 => byte == b'T',
            _ => byte.is_ascii_digit(),
        })
}

/// Every `.jsonl` file under the sessions root.
fn rollout_files(sessions_root: Option<&Path>) -> Vec<PathBuf> {
    let root = sessions_root
        .map(Path::to_path_buf)
        .or_else(|| codex_root(None).map(|root| root.join("sessions")));
    let mut found = CodexDiscovery::default();
    if let Some(root) = root {
        collect_jsonl(&root, &mut found);
    }
    found.files
}

fn codex_source(
    stage_id: String,
    thread: String,
    threads: &BTreeMap<String, ThreadLookup>,
) -> Option<Source> {
    let path = threads.get(&thread)?.rollout.clone()?;
    Some(Source {
        stage_id,
        key: format!("{CODEX_PREFIX}{thread}"),
        kind: AgentKind::Codex,
        path,
        agent_type: None,
    })
}

#[cfg(test)]
#[path = "discover_authorized_tests.rs"]
mod authorized_tests;
#[cfg(test)]
#[path = "discover_tests.rs"]
mod tests;
