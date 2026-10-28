//! A session's files: the record `session.json` (what a later server needs to find the
//! session's process, socket and children), the directory cleanup, and the readers the driver
//! tails the spool and transcripts with.

use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Transcripts;
use crate::fs::safe_fs::{safe_locked_write_in_workdir, safe_open_dirfd};
use crate::fs::safe_read::read_bounded;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::spool::{SpoolEvent, SpoolReader, Tail};
use crate::process::{terminate_verified, ProcessIdentity};
use crate::provider_data::resolve::{agent_id_from_path, list_agent_files};
use crate::tokens::tail::Line;

/// The record schema this build writes and reads.
pub const SCHEMA: u32 = 1;
const RECORD: &str = "session.json";
/// A record is a few hundred bytes; the bound only stops a planted giant file.
const MAX_RECORD_BYTES: usize = 256 * 1024;
/// Agent meta files hold a few ids.
const MAX_META_BYTES: usize = 64 * 1024;

/// One session as a later server finds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub schema: u32,
    pub id: String,
    pub kind: SessionKind,
    pub socket: String,
    pub tmux_session: String,
    /// `None` in the provisional record written before the spawn (px-state-11).
    pub pid: Option<u32>,
    pub start_time: Option<u64>,
    pub transcript_path: Option<PathBuf>,
    pub started_at: String,
    #[serde(default)]
    pub children: Vec<ChildRecord>,
}

/// A process the session's caller started for it (plan 11's Codex run).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildRecord {
    pub label: String,
    pub pid: u32,
    pub start_time: Option<u64>,
}

impl SessionRecord {
    /// The recorded identity of the Claude Code process; `None` for a provisional record.
    pub fn identity(&self) -> Option<ProcessIdentity> {
        self.pid.map(|pid| ProcessIdentity {
            pid,
            start_time: self.start_time,
        })
    }
}

impl ChildRecord {
    pub fn identity(&self) -> ProcessIdentity {
        ProcessIdentity {
            pid: self.pid,
            start_time: self.start_time,
        }
    }
}

/// Replace `session.json` whole, under an exclusive lock, refusing symlinks (mode 0600).
pub fn write_record(dir: &SessionDir, record: &SessionRecord) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(record)?;
    let fd = safe_open_dirfd(dir.path())?;
    safe_locked_write_in_workdir(fd.as_raw_fd(), Path::new(RECORD), &bytes)
        .with_context(|| format!("cannot write {}", dir.record().display()))
}

/// Read `session.json` bounded and no-follow.
pub fn read_record(dir: &SessionDir) -> anyhow::Result<SessionRecord> {
    let bytes = read_bounded(dir.path(), Path::new(RECORD), MAX_RECORD_BYTES)?;
    let record: SessionRecord = serde_json::from_slice(&bytes)
        .with_context(|| format!("cannot parse {}", dir.record().display()))?;
    anyhow::ensure!(
        record.schema == SCHEMA,
        "{} has schema {}, not {SCHEMA}",
        dir.record().display(),
        record.schema
    );
    Ok(record)
}

/// Every `sessions/<name>/` directory (symlinks and files skipped), sorted by name.
pub(super) fn session_dirs(sessions: &Path) -> Vec<(String, SessionDir)> {
    let Ok(entries) = std::fs::read_dir(sessions) else {
        return Vec::new();
    };
    let mut dirs: Vec<(String, SessionDir)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            Some((name, SessionDir::new(entry.path())))
        })
        .collect();
    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    dirs
}

/// Remove a session directory; one already gone is not an error. `remove_dir_all` never
/// follows a symlink inside the tree.
pub(super) fn remove_session_dir(dir: &SessionDir) -> anyhow::Result<()> {
    match std::fs::remove_dir_all(dir.path()) {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            Err(error).with_context(|| format!("cannot remove {}", dir.path().display()))
        }
        _ => Ok(()),
    }
}

/// SIGTERM each child whose identity still verifies; missing evidence signals nothing.
pub(super) fn terminate_children(children: &[ChildRecord]) {
    for child in children {
        if let Err(error) = terminate_verified(child.identity()) {
            tracing::warn!(label = %child.label, pid = child.pid, %error, "child not terminated");
        }
    }
}

/// `<transcript dir>/<session id>/subagents`, when neither directory is a symlink.
fn subagent_dir(main: &Path) -> Option<PathBuf> {
    let session = main.parent()?.join(main.file_stem()?);
    let dir = session.join("subagents");
    let plain = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir());
    (plain(&session) && plain(&dir)).then_some(dir)
}

pub(super) fn transcripts_of(main: Option<PathBuf>) -> Transcripts {
    let dir = main.as_deref().and_then(subagent_dir);
    let files = dir.map(|dir| list_agent_files(&dir)).unwrap_or_default();
    let subagents = files
        .into_iter()
        .map(|path| (agent_id_from_path(&path), path))
        .collect();
    Transcripts { main, subagents }
}

/// The driver's readers, per session id.
pub(super) type Feeds = HashMap<String, Feed>;

/// The text lines a transcript or token file gained; an oversized line is skipped.
fn read_text(tail: &mut Tail) -> Vec<String> {
    let mut lines = Vec::new();
    let read = tail.read_lines(|line| {
        if let Line::Text(text) = line {
            lines.push(text.to_owned());
        }
    });
    if let Err(error) = read {
        tracing::warn!(path = %tail.path().display(), %error, "cannot read a transcript");
    }
    lines
}

struct AgentFeed {
    tail: Tail,
    agent_id: String,
    spawned_by: Option<String>,
}

/// What the driver tails for one session.
pub(super) struct FeedPaths {
    pub(super) events: PathBuf,
    pub(super) transcript: Option<PathBuf>,
    pub(super) tokens: Vec<PathBuf>,
}

/// The lines and events one read produced.
pub(super) struct Batch {
    pub(super) main: Vec<String>,
    /// `(agent id, spawning tool_use_id, lines)`.
    pub(super) agents: Vec<(String, Option<String>, Vec<String>)>,
    pub(super) tokens: Vec<String>,
    pub(super) events: Vec<SpoolEvent>,
}

/// One line or event of a batch, for applying in time order.
pub(super) enum Entry<'a> {
    Main(&'a str),
    Agent(&'a str, Option<&'a str>, &'a str),
    Event(&'a SpoolEvent),
}

impl Batch {
    /// Records and hook events in time order, so an interrupt record never applies before the
    /// events that preceded it. Each stream keeps its own order (its key never goes back); a
    /// record and an event at the same time apply record first, so a Stop closes its turn after
    /// the turn's records joined it.
    pub(super) fn in_time_order(&self) -> Vec<Entry<'_>> {
        let mut keyed = Vec::new();
        let main = self
            .main
            .iter()
            .map(|line| (stamp_of(line), Entry::Main(line)));
        keyed.extend(monotonic(main));
        for (agent, by, lines) in &self.agents {
            let by = by.as_deref();
            let stream = lines
                .iter()
                .map(|line| (stamp_of(line), Entry::Agent(agent, by, line)));
            keyed.extend(monotonic(stream));
        }
        let events = self.events.iter();
        keyed.extend(monotonic(
            events.map(|event| (Some(event.received_at), Entry::Event(event))),
        ));
        keyed.sort_by_key(|(at, _)| *at);
        keyed.into_iter().map(|(_, entry)| entry).collect()
    }
}

/// Keys that never go back: a line without a time, or with an earlier one, keeps its place.
fn monotonic<'a>(
    entries: impl Iterator<Item = (Option<DateTime<Utc>>, Entry<'a>)>,
) -> Vec<(DateTime<Utc>, Entry<'a>)> {
    let mut floor = DateTime::<Utc>::MIN_UTC;
    let keyed = entries.map(|(at, entry)| {
        floor = at.map_or(floor, |at| at.max(floor));
        (floor, entry)
    });
    keyed.collect()
}

/// A transcript record's top-level `timestamp`.
fn stamp_of(line: &str) -> Option<DateTime<Utc>> {
    #[derive(Deserialize)]
    struct Stamp {
        timestamp: Option<DateTime<Utc>>,
    }
    serde_json::from_str::<Stamp>(line).ok()?.timestamp
}

pub(super) struct Feed {
    spool: SpoolReader,
    main: Option<Tail>,
    agents: BTreeMap<PathBuf, AgentFeed>,
    tokens: Vec<Tail>,
    /// Agent id to the `tool_use_id` of the Agent call whose result named it.
    spawners: HashMap<String, String>,
}

impl Feed {
    pub(super) fn new(events: PathBuf) -> Self {
        Feed {
            spool: SpoolReader::new(events),
            main: None,
            agents: BTreeMap::new(),
            tokens: Vec::new(),
            spawners: HashMap::new(),
        }
    }

    /// Follow a changed transcript path (the old file is no longer read) and new token files.
    pub(super) fn sync(&mut self, paths: &FeedPaths) {
        if self.main.as_ref().map(Tail::path) != paths.transcript.as_deref() {
            self.main = paths.transcript.clone().map(Tail::new);
            self.agents.clear();
        }
        for path in &paths.tokens {
            if !self.tokens.iter().any(|tail| tail.path() == path) {
                self.tokens.push(Tail::new(path.clone()));
            }
        }
    }

    /// The spool first: a record older than an event read here was written before it, so the
    /// transcript read that follows sees it.
    pub(super) fn read(&mut self) -> Batch {
        let events = self.spool.read_new();
        let main = self.main.as_mut().map(read_text).unwrap_or_default();
        for line in &main {
            note_spawner(line, &mut self.spawners);
        }
        let agents = self.read_agents();
        let tokens = self.tokens.iter_mut().flat_map(read_text).collect();
        Batch {
            main,
            agents,
            tokens,
            events,
        }
    }

    fn read_agents(&mut self) -> Vec<(String, Option<String>, Vec<String>)> {
        let Some(dir) = self
            .main
            .as_ref()
            .and_then(|main| subagent_dir(main.path()))
        else {
            return Vec::new();
        };
        for path in list_agent_files(&dir) {
            self.agents
                .entry(path.clone())
                .or_insert_with(|| AgentFeed {
                    agent_id: agent_id_from_path(&path),
                    tail: Tail::new(path),
                    spawned_by: None,
                });
        }
        let mut read = Vec::new();
        for feed in self.agents.values_mut() {
            if feed.spawned_by.is_none() {
                feed.spawned_by = spawning_tool(&dir, &feed.agent_id)
                    .or_else(|| self.spawners.get(&feed.agent_id).cloned());
            }
            let lines = read_text(&mut feed.tail);
            if !lines.is_empty() {
                read.push((feed.agent_id.clone(), feed.spawned_by.clone(), lines));
            }
        }
        read
    }
}

/// The spawning call's `toolUseId` from `agent-<id>.meta.json`, read bounded and no-follow.
fn spawning_tool(dir: &Path, agent_id: &str) -> Option<String> {
    let name = PathBuf::from(format!("agent-{agent_id}.meta.json"));
    let bytes = read_bounded(dir, &name, MAX_META_BYTES).ok()?;
    let meta: Value = serde_json::from_slice(&bytes).ok()?;
    meta.get("toolUseId")?.as_str().map(str::to_owned)
}

/// The fallback for a subagent without a meta file: the Agent tool result naming it.
fn note_spawner(line: &str, spawners: &mut HashMap<String, String>) {
    if !line.contains("\"agentId\"") {
        return;
    }
    let Ok(record) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let agent = record
        .pointer("/toolUseResult/agentId")
        .and_then(Value::as_str);
    let blocks = record.pointer("/message/content").and_then(Value::as_array);
    let tool_use = blocks
        .into_iter()
        .flatten()
        .find_map(|block| block.get("tool_use_id")?.as_str());
    if let (Some(agent), Some(tool_use)) = (agent, tool_use) {
        spawners.insert(agent.to_owned(), tool_use.to_owned());
    }
}
