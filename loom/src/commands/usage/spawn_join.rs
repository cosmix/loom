//! Joins hook spawn requests to Claude lifecycle starts.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde_json::Value;

const MAX_LEDGER_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) const AMBIGUOUS_MODEL: &str = "ambiguous";

type JoinKey = (String, String);
type SpawnGroups = HashMap<JoinKey, Vec<Spawn>>;
type StartGroups = HashMap<JoinKey, Vec<Start>>;

/// Requested models indexed by the Claude session which owns an agent.
#[derive(Default)]
pub(crate) struct RequestedModels {
    models: HashMap<(String, String), String>,
}

struct Spawn {
    ts: String,
    model: String,
}

struct Start {
    ts: String,
    agent_id: String,
    parent_session_id: String,
}

impl RequestedModels {
    pub(crate) fn load(work_dir: Option<&Path>) -> Self {
        let mut spawns = SpawnGroups::new();
        let mut starts = StartGroups::new();
        for stage_dir in stage_directories(work_dir) {
            collect_rows(&stage_dir, &mut spawns, &mut starts);
        }
        Self {
            models: join_groups(spawns, starts),
        }
    }

    pub(crate) fn requested_model(&self, agent_id: &str, parent_session_id: &str) -> Option<&str> {
        self.models
            .get(&(parent_session_id.to_owned(), agent_id.to_owned()))
            .map(String::as_str)
    }
}

fn collect_rows(stage_dir: &Path, spawns: &mut SpawnGroups, starts: &mut StartGroups) {
    for row in json_lines(&stage_dir.join("spawns.jsonl")) {
        let Some((key, spawn)) = Spawn::from_row(&row) else {
            continue;
        };
        spawns.entry(key).or_default().push(spawn);
    }
    for row in json_lines(&stage_dir.join("starts.jsonl")) {
        let Some((key, start)) = Start::from_row(&row) else {
            continue;
        };
        starts.entry(key).or_default().push(start);
    }
}

impl Spawn {
    fn from_row(row: &Value) -> Option<(JoinKey, Self)> {
        Some((
            (
                string_field(row, "session_id")?,
                string_field(row, "agent_type")?,
            ),
            Self {
                ts: string_field(row, "ts")?,
                model: string_field(row, "model")?,
            },
        ))
    }
}

impl Start {
    fn from_row(row: &Value) -> Option<(JoinKey, Self)> {
        Some((
            (
                string_field(row, "loom_session_id")?,
                string_field(row, "agent_type")?,
            ),
            Self {
                ts: string_field(row, "ts")?,
                agent_id: string_field(row, "agent_id")?,
                parent_session_id: string_field(row, "parent_session_id")?,
            },
        ))
    }
}

fn string_field(row: &Value, field: &str) -> Option<String> {
    row.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn join_groups(mut spawns: SpawnGroups, starts: StartGroups) -> HashMap<(String, String), String> {
    let mut models = HashMap::new();
    for (key, mut start_group) in starts {
        let Some(mut spawn_group) = spawns.remove(&key) else {
            continue;
        };
        spawn_group.sort_by(|left, right| left.ts.cmp(&right.ts));
        start_group.sort_by(|left, right| left.ts.cmp(&right.ts));
        join_group(&spawn_group, &start_group, &mut models);
    }
    models
}

fn join_group(spawns: &[Spawn], starts: &[Start], models: &mut HashMap<(String, String), String>) {
    let distinct_models: HashSet<&str> = spawns.iter().map(|spawn| spawn.model.as_str()).collect();
    // An empty model counts as a value of its own: that agent inherited the
    // parent's model, so it must not take the group's named one.
    if distinct_models.len() == 1 {
        let Some(model) = distinct_models.into_iter().next() else {
            return;
        };
        record_all(starts, model, models);
    } else if spawns.len() == starts.len() {
        record_paired(spawns, starts, models);
    } else {
        record_all(starts, AMBIGUOUS_MODEL, models);
    }
}

fn record_all(starts: &[Start], model: &str, models: &mut HashMap<(String, String), String>) {
    if model.is_empty() {
        return;
    }
    for start in starts {
        models.insert(
            (start.parent_session_id.clone(), start.agent_id.clone()),
            model.to_owned(),
        );
    }
}

fn record_paired(
    spawns: &[Spawn],
    starts: &[Start],
    models: &mut HashMap<(String, String), String>,
) {
    for (spawn, start) in spawns.iter().zip(starts) {
        let model = if spawn_has_conflicting_timestamp(spawn, spawns) {
            AMBIGUOUS_MODEL
        } else {
            &spawn.model
        };
        if model.is_empty() {
            continue;
        }
        models.insert(
            (start.parent_session_id.clone(), start.agent_id.clone()),
            model.to_owned(),
        );
    }
}

fn spawn_has_conflicting_timestamp(spawn: &Spawn, spawns: &[Spawn]) -> bool {
    spawns
        .iter()
        .any(|other| other.ts == spawn.ts && other.model != spawn.model)
}

fn stage_directories(work_dir: Option<&Path>) -> Vec<PathBuf> {
    let Some(work_dir) = work_dir else {
        return Vec::new();
    };
    let root = work_dir.join("subagents");
    if !plain_directory(&root) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut stage_dirs: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| plain_directory(path))
        .collect();
    stage_dirs.sort();
    stage_dirs
}

fn plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_dir() && !metadata.file_type().is_symlink())
}

fn json_lines(path: &Path) -> Vec<Value> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Vec::new();
    };
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_LEDGER_BYTES
    {
        return Vec::new();
    }
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    else {
        return Vec::new();
    };
    let mut content = String::new();
    let mut limited = file.take(MAX_LEDGER_BYTES);
    if limited.read_to_string(&mut content).is_err() {
        return Vec::new();
    }
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(Value::is_object)
        .collect()
}

#[cfg(test)]
#[path = "spawn_join_tests.rs"]
mod tests;
